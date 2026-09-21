//! MIDI text has no encoding declaration. Prefer UTF-8, then legacy Chinese.
pub(super) fn decode_name(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.trim_matches('\0').to_owned();
    }
    #[cfg(windows)]
    if let Some(text) = decode_chinese(bytes) {
        return text.trim_matches('\0').to_owned();
    }
    "未识别名称".into()
}

#[cfg(windows)]
fn decode_chinese(bytes: &[u8]) -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MultiByteToWideChar(
            code_page: u32,
            flags: u32,
            source: *const u8,
            source_len: i32,
            destination: *mut u16,
            destination_len: i32,
        ) -> i32;
    }
    let len = i32::try_from(bytes.len()).ok()?;
    // GB18030 also accepts GBK/GB2312; strict decoding rejects malformed bytes.
    let size =
        unsafe { MultiByteToWideChar(54936, 8, bytes.as_ptr(), len, std::ptr::null_mut(), 0) };
    if size <= 0 {
        return None;
    }
    let mut wide = vec![0u16; size as usize];
    let written =
        unsafe { MultiByteToWideChar(54936, 8, bytes.as_ptr(), len, wide.as_mut_ptr(), size) };
    if written != size {
        return None;
    }
    String::from_utf16(&wide).ok()
}
