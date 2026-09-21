use super::fit_part;
use crate::models::{Note, Options};

/// Collapse only successful, identical final note sequences. Keep the selected
/// method as its group's representative, without changing persisted options.
pub fn distinct_melody_modes(notes: &[Note], options: &Options) -> Vec<&'static str> {
    let mut results: Vec<Option<Vec<Note>>> = Vec::new();
    let mut choices = Vec::new();
    for mode in ["sustain", "highest", "continuous"] {
        let candidate = Options {
            melody_mode: mode.into(),
            ..options.clone()
        };
        let result = fit_part(notes, &candidate).ok().map(|(notes, _)| notes);
        let duplicate = result.as_ref().and_then(|notes| {
            results
                .iter()
                .position(|other| other.as_ref() == Some(notes))
        });
        if let Some(index) = duplicate {
            if mode == options.melody_mode {
                choices[index] = mode;
            }
        } else {
            choices.push(mode);
            results.push(result);
        }
    }
    choices
}
