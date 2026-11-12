//! `_core.extract(..., extra={...})`: the settings.cfg sizes and per-call filters, keyed like trafilatura's
//! `Extractor` attributes (htmldate flags as in `date_params`). Values arrive validated by python/trafilatura_rs.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use trafilatura_rs::trafilatura::settings::Options;

pub fn apply(opts: &mut Options, extra: &Bound<'_, PyDict>) -> PyResult<()> {
    for (key, value) in extra.iter() {
        let key: String = key.extract()?;
        match key.as_str() {
            "min_extracted_size" => opts.min_extracted_size = value.extract()?,
            "min_output_size" => opts.min_output_size = value.extract()?,
            "min_output_comm_size" => opts.min_output_comm_size = value.extract()?,
            "min_extracted_comm_size" => opts.min_extracted_comm_size = value.extract()?,
            "min_duplcheck_size" => opts.min_duplcheck_size = value.extract()?,
            "max_repetitions" => opts.max_repetitions = value.extract()?,
            "max_tree_size" => opts.max_tree_size = value.extract()?,
            "only_with_metadata" => opts.only_with_metadata = value.extract()?,
            "url_blacklist" => opts.url_blacklist = value.extract()?,
            "author_blacklist" => opts.author_blacklist = value.extract()?,
            "prune_xpath" => opts.prune_xpath = value.extract()?,
            "original_date" => opts.original_date = value.extract()?,
            "extensive_search" => opts.extensive_date_search = value.extract()?,
            other => return Err(PyValueError::new_err(format!("unknown extra option {other:?}"))),
        }
    }
    Ok(())
}
