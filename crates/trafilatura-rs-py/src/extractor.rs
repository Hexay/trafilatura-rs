//! `trafilatura_rs._core.Extractor`: str input with its own dedup state.
use crate::{options, request, to_py_err, Engine};
use pyo3::prelude::*;
use trafilatura_rs::trafilatura::deduplication::Lru;
use trafilatura_rs::trafilatura::settings::Options;

/// An extractor with its own dedup state, independent of the module-level one.
#[pyclass(subclass)]
pub(crate) struct Extractor {
    engine: Engine,
    opts: Options,
    lru: Lru,
}

#[pymethods]
impl Extractor {
    #[new]
    #[pyo3(signature = (engine, favor_precision=false, favor_recall=false, include_tables=true, deduplicate=false,
        no_fallback=false, include_comments=true, output_format="txt", with_metadata=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        engine: &str,
        favor_precision: bool,
        favor_recall: bool,
        include_tables: bool,
        deduplicate: bool,
        no_fallback: bool,
        include_comments: bool,
        output_format: &str,
        with_metadata: bool,
    ) -> PyResult<Self> {
        let (engine, opts) = options(
            engine,
            favor_precision,
            favor_recall,
            include_tables,
            deduplicate,
            no_fallback,
            include_comments,
            output_format,
            with_metadata,
        )?;
        Ok(Extractor { engine, opts, lru: Lru::default() })
    }

    #[pyo3(signature = (html, url=None, record_id=None, max_date=None))]
    fn extract(
        &mut self,
        py: Python<'_>,
        html: &str,
        url: Option<&str>,
        record_id: Option<&str>,
        max_date: Option<&str>,
    ) -> PyResult<Option<String>> {
        let Extractor { engine, opts, lru } = self;
        let req = request(url, record_id, max_date);
        py.detach(|| engine.extract(html, opts, &req, lru)).map_err(to_py_err)
    }
}
