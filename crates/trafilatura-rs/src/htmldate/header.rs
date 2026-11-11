//! htmldate/core.py `examine_header`: `<meta>` cues.
use super::extractors::try_date_expr_opts;
use super::searches::extract_url_date;
use super::validators::is_valid_date_str;
use super::{cslice, Env};
use crate::dom::{Node, Result};
use crate::py::datetime::DateTime;
use crate::py::int::py_int;

const DATE_ATTRIBUTES: &[&str] = &[
    "analyticsattributes.articledate",
    "article.created",
    "article_date_original",
    "article:post_date",
    "article.published",
    "article:published",
    "article:published_date",
    "article:published_time",
    "article:publicationdate",
    "bt:pubdate",
    "citation_date",
    "citation_publication_date",
    "content_create_date",
    "created",
    "cxenseparse:recs:publishtime",
    "date",
    "date_created",
    "date_published",
    "datecreated",
    "dateposted",
    "datepublished",
    "dc.date",
    "dc.created",
    "dc.date.created",
    "dc.date.issued",
    "dc.date.publication",
    "dcsext.articlefirstpublished",
    "dcterms.created",
    "dcterms.date",
    "dcterms.issued",
    "dc:created",
    "dc:date",
    "displaydate",
    "doc_date",
    "field-name-post-date",
    "gentime",
    "mediator_published_time",
    "meta",
    "og:article:published",
    "og:article:published_time",
    "og:datepublished",
    "og:pubdate",
    "og:publish_date",
    "og:published_time",
    "og:question:published_time",
    "og:regdate",
    "originalpublicationdate",
    "parsely-pub-date",
    "pdate",
    "ptime",
    "pubdate",
    "publishdate",
    "publish_date",
    "publish_time",
    "publish-date",
    "published-date",
    "published_date",
    "published_time",
    "publisheddate",
    "publication_date",
    "rbpubdate",
    "release_date",
    "rnews:datepublished",
    "sailthru.date",
    "shareaholic:article_published_time",
    "timestamp",
    "twt-published-at",
    "video:release_date",
    "vr:published_time",
];
const NAME_MODIFIED: &[&str] = &["lastdate", "lastmod", "lastmodified", "last-modified", "modified", "utime"];
const PROPERTY_MODIFIED: &[&str] = &[
    "article:modified",
    "article:modified_date",
    "article:modified_time",
    "article:post_modified",
    "bt:moddate",
    "datemodified",
    "dc.modified",
    "dcterms.modified",
    "lastmodified",
    "modified_time",
    "modificationdate",
    "og:article:modified_time",
    "og:modified_time",
    "og:updated_time",
    "release_date",
    "revision_date",
    "updated_time",
];
const ITEMPROP_ATTRS_ORIGINAL: &[&str] = &["datecreated", "datepublished", "pubyear"];
const ITEMPROP_ATTRS_MODIFIED: &[&str] = &["datemodified", "dateupdate"];

fn lower_attr(elem: Node, name: &str) -> String {
    elem.get(name).unwrap_or_default().to_lowercase()
}

/// `examine_header`.
pub(super) fn examine_header(env: &Env, tree: Node) -> Result<Option<String>> {
    let (mut headerdate, mut reserve) = (None, None);
    let tryfunc = |s: Option<&str>| try_date_expr_opts(env, s);
    for elem in tree.findall(".//meta") {
        if !elem.has_attr("content") && !elem.has_attr("datetime") {
            continue;
        }
        let content = elem.get("content");
        let content = content.as_deref();
        if elem.has_attr("name") {
            let attribute = lower_attr(elem, "name");
            if attribute == "og:url" {
                reserve = extract_url_date(env, content)?.or(reserve);
            } else if DATE_ATTRIBUTES.contains(&attribute.as_str()) {
                headerdate = tryfunc(content)?;
            } else if NAME_MODIFIED.contains(&attribute.as_str()) {
                if !env.original {
                    headerdate = tryfunc(content)?;
                } else {
                    reserve = tryfunc(content)?.or(reserve);
                }
            }
        } else if elem.has_attr("property") {
            let attribute = lower_attr(elem, "property");
            let attribute = attribute.as_str();
            if DATE_ATTRIBUTES.contains(&attribute) || PROPERTY_MODIFIED.contains(&attribute) {
                if let Some(attempt) = tryfunc(content)? {
                    if (if env.original { DATE_ATTRIBUTES } else { PROPERTY_MODIFIED }).contains(&attribute) {
                        headerdate = Some(attempt);
                    } else {
                        reserve = Some(attempt);
                    }
                }
            }
        } else if elem.has_attr("itemprop") {
            let attribute = lower_attr(elem, "itemprop");
            let attribute = attribute.as_str();
            if ITEMPROP_ATTRS_ORIGINAL.contains(&attribute) || ITEMPROP_ATTRS_MODIFIED.contains(&attribute) {
                let datetime = elem.get("datetime").filter(|d| !d.is_empty());
                if let Some(attempt) = tryfunc(datetime.as_deref().or(content))? {
                    let wanted = if env.original { ITEMPROP_ATTRS_ORIGINAL } else { ITEMPROP_ATTRS_MODIFIED };
                    if wanted.contains(&attribute) {
                        headerdate = Some(attempt);
                    }
                }
            } else if attribute == "copyrightyear" {
                if let Some(content) = content {
                    let attempt = format!("{content}-01-01");
                    if is_valid_date_str(&attempt, env)? {
                        reserve = Some(DateTime::ymd(py_int(cslice(&attempt, 0, 4))?, 1, 1)?.strftime(env.format));
                    }
                }
            }
        } else if elem.has_attr("pubdate") {
            if lower_attr(elem, "pubdate") == "pubdate" {
                headerdate = tryfunc(content)?;
            }
        } else if elem.has_attr("http-equiv") {
            let attribute = lower_attr(elem, "http-equiv");
            if attribute == "date" || attribute == "last-modified" {
                let attempt = tryfunc(content)?;
                // "date" is original, "last-modified" is updated
                if (attribute == "date") == env.original {
                    headerdate = attempt;
                } else {
                    reserve = attempt.or(reserve);
                }
            }
        }
        if headerdate.is_some() {
            break;
        }
    }
    Ok(headerdate.or(reserve))
}
