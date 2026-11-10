//! trafilatura/settings.py `Extractor` with settings.cfg defaults (metadata/download fields omitted).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Recall,
    Precision,
    Balanced,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub fast: bool,
    pub focus: Focus,
    pub comments: bool,
    pub formatting: bool,
    pub links: bool,
    pub images: bool,
    pub tables: bool,
    pub dedup: bool,
    pub min_extracted_size: usize,
    pub min_output_size: usize,
    pub min_output_comm_size: usize,
    pub min_extracted_comm_size: usize,
    pub min_duplcheck_size: usize,
    pub max_repetitions: usize,
    pub max_tree_size: Option<usize>,
}

impl Default for Options {
    /// `Extractor()` defaults: balanced focus, comments and tables on, txt output.
    fn default() -> Options {
        Options {
            fast: false,
            focus: Focus::Balanced,
            comments: true,
            formatting: false,
            links: false,
            images: false,
            tables: true,
            dedup: false,
            min_extracted_size: 250,
            min_output_size: 1,
            min_output_comm_size: 1,
            min_extracted_comm_size: 1,
            min_duplcheck_size: 100,
            max_repetitions: 2,
            max_tree_size: None,
        }
    }
}

impl Options {
    /// datatrove's `Trafilatura(favour_precision=True)` call: `extract(favor_precision=True,
    /// include_comments=False, deduplicate=True)`.
    pub fn fineweb() -> Options {
        Options { focus: Focus::Precision, comments: false, dedup: true, ..Options::default() }
    }
}
