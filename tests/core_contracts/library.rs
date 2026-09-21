use super::support::*;

#[test]
fn seed_song_selects_original_bassoon_melody() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/欢乐颂.mid");
    let (parts, names) = read_midi(&path).unwrap();
    let (got, report) = prepare(&parts, &names, &Options::default()).unwrap();
    assert_eq!(report["melody_notes"], 94);
    assert_eq!(report["track_name"], "Bassoon");
    assert_eq!(report["transpose_semitones"], 12);
    assert_eq!(
        got.iter().take(4).map(|n| n.pitch).collect::<Vec<_>>(),
        [66, 66, 67, 69]
    );
}
#[test]
fn all_bundled_midis_preserve_score_and_event_contracts_in_every_mode() {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
    let catalog: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(samples.join("catalog.json")).unwrap()).unwrap();
    let catalog: BTreeMap<_, _> = catalog
        .into_iter()
        .map(|row| (row["file"].as_str().unwrap().to_owned(), row))
        .collect();
    let mut paths: Vec<_> = fs::read_dir(&samples)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| ["mid", "midi", "kar", "rmi"].contains(&s.to_lowercase().as_str()))
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "曲库缺失，不能将空集合视为通过");
    let output = tempfile::tempdir().unwrap();
    let exported = output.path().join("roundtrip.mid");
    let mut variants = 0;
    let mut raw_notes = 0;
    let mut verified_catalog = 0;
    let mut matched_default = 0;
    let mut matched_preset = 0;
    let mut differences = Vec::new();
    for path in &paths {
        let file = path.file_name().unwrap().to_string_lossy();
        let (parts, names) =
            read_midi(path).unwrap_or_else(|error| panic!("{file}: MIDI 解析失败: {error}"));
        let original = parts.clone();
        raw_notes += parts.values().map(Vec::len).sum::<usize>();
        let row = catalog.get(file.as_ref()).filter(|row| {
            row["sha256"].as_str()
                == Some(format!("{:x}", Sha256::digest(fs::read(path).unwrap())).as_str())
        });
        if let Some(row) = row {
            verified_catalog += 1;
            let (_, report) = prepare(&parts, &names, &Options::default()).unwrap();
            let comparison = |expected: &serde_json::Value, actual: &serde_json::Value| {
                expected.as_object().is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|(key, value)| actual.get(key) == Some(value))
                })
            };
            if comparison(&row["default_report"], &report) {
                matched_default += 1;
            } else {
                differences.push(json!({"file":file,"kind":"default_report","actual":report,"catalog":row["default_report"]}));
            }
            let preset: Options =
                serde_json::from_value(row.get("options").cloned().unwrap_or(json!({}))).unwrap();
            let (_, report) = prepare(&parts, &names, &preset).unwrap();
            if comparison(&row["report"], &report) {
                matched_preset += 1;
            } else {
                differences.push(json!({"file":file,"kind":"preset_report","actual":report,"catalog":row["report"]}));
            }
        }
        for mode in ["highest", "sustain", "continuous"] {
            for phrase_octave in [false, true] {
                let options = Options {
                    melody_mode: mode.into(),
                    phrase_octave,
                    ..Options::default()
                };
                let (notes, report) = prepare(&parts, &names, &options).unwrap_or_else(|error| {
                    panic!("{file}/{mode}/{phrase_octave}: 提取失败: {error}")
                });
                assert_eq!(
                    normalize_score_notes(&notes).unwrap(),
                    notes,
                    "{file}/{mode}: 原谱未规范化"
                );
                assert_eq!(
                    report["melody_notes"].as_u64().unwrap() as usize,
                    notes.len()
                );
                let canonical = notes.clone();
                for skip in [false, true] {
                    let (performance, _, _) = compress_long_rests(&notes, skip);
                    let (events, _) = build_events(&performance).unwrap_or_else(|error| {
                        panic!("{file}/{mode}/{phrase_octave}/{skip}: 编排失败: {error}")
                    });
                    let actual = decode_events(&events).unwrap();
                    assert_eq!(
                        actual.iter().map(|n| n.pitch).collect::<Vec<_>>(),
                        notes.iter().map(|n| n.pitch).collect::<Vec<_>>(),
                        "{file}/{mode}: 演奏音高发生变化"
                    );
                    assert!(actual.windows(2).all(|notes| notes[0].end < notes[1].start));
                    write_midi(&actual, &exported).unwrap();
                    let (roundtrip, _) = read_midi(&exported).unwrap();
                    assert_eq!(roundtrip[&(0, 0)].len(), actual.len());
                    for (left, right) in roundtrip[&(0, 0)].iter().zip(&actual) {
                        assert_eq!((left.pitch, left.velocity), (right.pitch, right.velocity));
                        approx(left.start, right.start);
                        approx(left.end, right.end);
                    }
                    let anchors = playback_anchors(&notes, &actual);
                    let map = TimeMap::new(anchors.clone());
                    let inverse = TimeMap::new(anchors.into_iter().map(|(a, b)| (b, a)));
                    for (logical, physical) in notes.iter().zip(&actual) {
                        approx(map.map(logical.start), physical.start);
                        approx(inverse.map(physical.start), logical.start);
                    }
                    assert_eq!(notes, canonical, "{file}/{mode}: 编排修改了原始时间");
                    variants += 1;
                }
            }
        }
        assert_eq!(parts, original, "{file}: 提取修改了原始 MIDI");
    }
    println!(
        "CORPUS_AUDIT {}",
        json!({"midi_files":paths.len(),"raw_notes":raw_notes,"variants":variants,"hash_verified_catalog_entries":verified_catalog,"catalog_default_reports_matching":matched_default,"catalog_preset_reports_matching":matched_preset,"catalog_differences":differences})
    );
}
#[test]
fn bundled_library_matches_archived_algorithm_evaluation_statistics() {
    // Scalar regression fixtures from the original melody-evaluation.json. Hashes
    // pin the actual MIDI corpus. These are compatibility checks, not accuracy labels.
    use sha2::{Digest, Sha256};
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
    let cases: &[(&str, &str, usize, (usize, u8), [usize; 3], [usize; 3])] = &[
        (
            "Bad Apple!!（坏家伙）- 东方Project (Remix Version).mid",
            "c568a5304b3483668c93ce05e44b26c5e4686bcbf067ddd88275b46045b843a3",
            5127,
            (3, 4),
            [652, 0, 0],
            [652, 0, 0],
        ),
        (
            "Flower Dance.mid",
            "182e5f1e81ed9692a7647be898f78a84a65acf37f344d1e3b1c1af1f67012f75",
            3522,
            (4, 0),
            [161, 0, 0],
            [161, 0, 0],
        ),
        (
            "HE IS A PIRATE《加勒比海盗》背景音乐.mid",
            "f292307d85119cea5b7008618cebd5dce7f2c3f047a96ab66859416ed6744db3",
            242,
            (1, 0),
            [242, 0, 0],
            [242, 0, 0],
        ),
        (
            "See You Again  主旋律  纯净钢琴单轨.mid",
            "5253cb1b4bd39af94e1781160aaaf50ac197e48c63de544dbc9b4f76fc43e597",
            106,
            (1, 0),
            [106, 0, 0],
            [104, 0, 0],
        ),
        (
            "一直很安静.mid",
            "a52f4b1714ab2b4ef57a0ae3ac183353c620a144cee4545f33d0225b7ea57a70",
            971,
            (2, 2),
            [269, 0, 0],
            [269, 0, 0],
        ),
        (
            "中国人会飞（副歌片段）.mid",
            "964f7a734eb3144e4a297940c35788d9ddf87cd8bfdf5f5a662e15188be6b929",
            30,
            (0, 0),
            [30, 0, 0],
            [30, 0, 0],
        ),
        (
            "友谊地久天长.mid",
            "efeb64292fac9151374196d3d0bf64f26af01f5dd2992bea156b8b8ac444e089",
            388,
            (1, 0),
            [116, 0, 0],
            [116, 0, 0],
        ),
        (
            "喀秋莎.mid",
            "9b8fed64ed6f4f5d9739e2b576cb2c3768406548f79b943cb1aac74221a307e4",
            1460,
            (1, 0),
            [236, 0, 0],
            [236, 0, 0],
        ),
        (
            "国际歌.mid",
            "9d49fe18b8424b45f122d1fd26ad8d640bd22e83f188ebd8b09c3239d670d5cd",
            250,
            (1, 0),
            [250, 0, 0],
            [250, 0, 0],
        ),
        (
            "夜曲-周杰伦【UTAU】.mid",
            "a1bd236c2b02b7258e4a74bdafb273621315dd08ee72c0724bd8e25c4c2c55ac",
            575,
            (6, 4),
            [575, 0, 0],
            [575, 0, 0],
        ),
        (
            "太阳照常升起.mid",
            "7fe56162d4e77a12106070a287104a5770128c9bfc04d09197774fdd8841744c",
            56,
            (1, 0),
            [56, 0, 0],
            [56, 0, 0],
        ),
        (
            "太阳照常升起（主题片段）.mid",
            "c9f718d89f948abffbecaf481ded65ce31c6436d63c166769d52370fe76471ed",
            56,
            (0, 0),
            [56, 0, 0],
            [56, 0, 0],
        ),
        (
            "奇异恩典.mid",
            "8d2757f8b329473e33e7b804def7b45332fbb79834646f82055c8084bd3b6040",
            456,
            (1, 1),
            [80, 0, 0],
            [80, 0, 0],
        ),
        (
            "小星星.mid",
            "ff7a8e0a947db744a7ef615b48571a23923b7e69b9d17514e07e13df74a3e1aa",
            90,
            (1, 0),
            [42, 0, 0],
            [42, 0, 0],
        ),
        (
            "得吃小曲.mid",
            "cb25fffbd613276eb57b80f6cc0e7b7ace7e64cf603467a568d556332d279575",
            489,
            (1, 0),
            [482, 0, 0],
            [339, 0, 0],
        ),
        (
            "得吃的小曲.mid",
            "b3416cb64ff512d1bbc86b4d0ed101558c5db64002c166b9418bea74fef9980d",
            321,
            (0, 0),
            [321, 0, 0],
            [321, 0, 0],
        ),
        (
            "念张师 主旋律.mid",
            "6f14ca8df8b630d239e4d0e6708d214fb3ae80dd1370cb2dd4940ee720d7db87",
            387,
            (1, 0),
            [387, 0, 0],
            [387, 0, 0],
        ),
        (
            "念张师.mid",
            "481e45afd4d2cfa73d6c76571c06d38d96bc356d97a9c12bf09898edad10bf77",
            387,
            (0, 0),
            [387, 0, 0],
            [387, 0, 0],
        ),
        (
            "斯卡布罗集市.mid",
            "f5a956bb4c29cefefe02d6a598c52a2588e2bd609cb7e7224e309d0d086bc19c",
            199,
            (1, 1),
            [66, 0, 0],
            [66, 0, 0],
        ),
        (
            "春日影.mid",
            "4ea4dba01b11355866356d77575f4ef50de02c2abed45b808494d7d6fda1902e",
            1490,
            (0, 0),
            [660, 0, 0],
            [660, 0, 0],
        ),
        (
            "晴天.mid",
            "436096d1f2d6edc17758bef4aba896766a763b28d4ffe6f2a0a528c55c6d3ea4",
            230,
            (1, 0),
            [230, 0, 0],
            [230, 0, 0],
        ),
        (
            "未闻花名.mid",
            "a18aea2f873a9e565fe775ef143ea9ed66ea657f39ba67fcdbd3b264996a35b6",
            1997,
            (1, 0),
            [599, 0, 0],
            [599, 0, 0],
        ),
        (
            "欢乐颂.mid",
            "cf45d07fbba2ff0bbc88851bad96df340dabcaa14eca24bc5b220f9293fdefd6",
            452,
            (1, 1),
            [94, 0, 0],
            [94, 0, 0],
        ),
        (
            "死别.mid",
            "eddda2d4f661de8ce2e43437495eed74d47092190d40b866ac7ba4b9b5a53602",
            1311,
            (0, 2),
            [440, 0, 0],
            [440, 0, 0],
        ),
        (
            "洛克王国-人鱼湾.mid",
            "8b10058f8d4384b4dbc410a791aa87fef8016b9a9c6bb3c119487a7e99322315",
            154,
            (1, 0),
            [59, 1, 5],
            [53, 0, 0],
        ),
        (
            "父亲筷子兄弟.mid",
            "f9a131126c69eafed8ea9d646f77a724cf8b7e31c5f1c308220dd1dbd7be26c5",
            461,
            (0, 0),
            [461, 0, 0],
            [461, 0, 0],
        ),
        (
            "红凯的小曲.mid",
            "86d06237f3fd231bea134f996f4a15621d032305896eea7a40b53c79972d8c90",
            388,
            (1, 0),
            [388, 0, 0],
            [388, 0, 0],
        ),
        (
            "绿袖子.mid",
            "2c96784f39c5664015f453b71717625350c8cff9de15ce46868eacb78ce777e5",
            492,
            (1, 0),
            [144, 0, 0],
            [144, 0, 0],
        ),
        (
            "花海 周杰伦 .mid",
            "3b4fd5bb315a10e5a79d1aacbd6c6ec968d520eca5ab4252f693c4b39b0e27d1",
            1901,
            (2, 0),
            [422, 0, 0],
            [416, 0, 0],
        ),
        (
            "贝加尔湖畔.mid",
            "35e66b374a446a84341239d6b2a89b0fbbc200307ba0c3d123b1982025018207",
            228,
            (1, 0),
            [228, 0, 0],
            [228, 0, 0],
        ),
        (
            "青花瓷.mid",
            "c424d2da9fa0b51c19205fb90d4710787883b23b8b3bb84b697fc1a03f7ba1b3",
            1355,
            (1, 0),
            [557, 0, 0],
            [558, 0, 0],
        ),
        (
            "鸟之诗.mid",
            "4a4607b8f7b8ac46701e8a0168f4009c3702aab64daa7d5d2595af0b1134bae3",
            260,
            (1, 0),
            [260, 0, 0],
            [260, 0, 0],
        ),
    ];
    for &(file, digest, count, key, sustain, continuous) in cases {
        let path = samples.join(file);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
            digest,
            "原始曲目已经改变: {file}"
        );
        let (parts, names) = read_midi(&path).unwrap();
        assert_eq!(
            parts.values().map(Vec::len).sum::<usize>(),
            count,
            "{file}: 原始 MIDI 解析数量改变"
        );
        assert_eq!(
            rank_parts(&parts, &names)[0].0,
            key,
            "{file}: 推荐声部与历史算法验收不一致"
        );
        for (mode, expected) in [("sustain", sustain), ("continuous", continuous)] {
            let options = Options {
                melody_mode: mode.into(),
                phrase_octave: true,
                ..Options::default()
            };
            let (notes, report) = prepare(&parts, &names, &options).unwrap();
            assert_eq!(
                [
                    notes.len(),
                    report["dropped_out_of_range"].as_u64().unwrap() as usize,
                    report["phrase_adjusted_notes"].as_u64().unwrap() as usize
                ],
                expected,
                "{file}/{mode}: 与历史算法验收不一致"
            );
        }
    }
    println!(
        "ARCHIVED_COMPATIBILITY {} files, {} mode evaluations matched",
        cases.len(),
        cases.len() * 2
    );
}
#[test]
fn scarborough_current_ranker_selects_flute_despite_stale_catalog_default_report() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/斯卡布罗集市.mid");
    let (parts, names) = read_midi(&path).unwrap();
    assert_eq!(rank_parts(&parts, &names)[0].0, (1, 1));
    let (notes, report) = prepare(&parts, &names, &Options::default()).unwrap();
    assert_eq!(notes.len(), 66);
    assert_eq!(report["track_name"], "Flute");
    assert_eq!(report["transpose_semitones"], -12);
}
