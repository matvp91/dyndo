#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Subtitle {
    pub cues: Vec<Cue>,
}

impl Subtitle {
    pub fn duration(&self) -> u32 {
        self.cues.iter().map(|cue| cue.end).max().unwrap_or(0)
    }
}
