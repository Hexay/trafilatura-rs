//! trafilatura/settings.py `Extractor` with settings.cfg defaults (metadata/download fields omitted).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Recall,
    Precision,
    Balanced,
}

/// `output_format` values the port covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Txt,
    Markdown,
    Json,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub format: Format,
    pub with_metadata: bool,
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
    pub only_with_metadata: bool,
    pub url_blacklist: Vec<String>,
    /// Already lowercased with Python's `str.lower()`.
    pub author_blacklist: Vec<String>,
    /// `prune_xpath`, applied to the loaded tree before extraction.
    pub prune_xpath: Vec<String>,
    /// htmldate's `original_date` and `extensive_search` (`date_params`).
    pub original_date: bool,
    pub extensive_date_search: bool,
}

impl Default for Options {
    /// `Extractor()` defaults: balanced focus, comments and tables on, txt output.
    fn default() -> Options {
        Options {
            format: Format::Txt,
            with_metadata: false,
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
            only_with_metadata: false,
            url_blacklist: Vec::new(),
            author_blacklist: Vec::new(),
            prune_xpath: Vec::new(),
            original_date: true,
            extensive_date_search: true,
        }
    }
}

impl Options {
    /// datatrove's `Trafilatura(favour_precision=True)` call: `extract(favor_precision=True,
    /// include_comments=False, deduplicate=True)`.
    pub fn fineweb() -> Options {
        Options { focus: Focus::Precision, comments: false, dedup: true, ..Options::default() }
    }

    /// `Extractor(output_format=...)`: markdown implies `formatting`.
    pub fn with_format(self, format: Format) -> Options {
        Options { format, formatting: self.formatting || format == Format::Markdown, ..self }
    }
}
