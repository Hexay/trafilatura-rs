//! trafilatura/settings.py `Document`. Metadata fields hold Python values: JSON-LD can put lists or dicts where
//! strings are expected, and they are printed as Python would.
use crate::dom::Node;
use crate::py::value::Value;

#[derive(Default)]
pub struct Document {
    pub title: Value,
    pub author: Value,
    pub url: Value,
    pub hostname: Value,
    pub description: Value,
    pub sitename: Value,
    pub date: Value,
    pub categories: Value,
    pub tags: Value,
    pub fingerprint: Value,
    pub id: Value,
    pub license: Value,
    pub body: Option<Node>,
    /// With include_comments, the extracted comments.
    pub commentsbody: Option<Node>,
    pub raw_text: Value,
    pub language: Value,
    pub image: Value,
    pub pagetype: Value,
    pub filedate: Value,
}

impl Document {
    /// The header fields of `determine_returnstring`, in order.
    pub fn header_fields(&self) -> [(&'static str, &Value); 12] {
        [
            ("title", &self.title),
            ("author", &self.author),
            ("url", &self.url),
            ("hostname", &self.hostname),
            ("description", &self.description),
            ("sitename", &self.sitename),
            ("date", &self.date),
            ("categories", &self.categories),
            ("tags", &self.tags),
            ("fingerprint", &self.fingerprint),
            ("id", &self.id),
            ("license", &self.license),
        ]
    }

    /// Mutable access to every slot `clean_and_trim` visits (all but body/commentsbody), in slot order.
    pub fn slots_mut(&mut self) -> [&mut Value; 17] {
        [
            &mut self.title,
            &mut self.author,
            &mut self.url,
            &mut self.hostname,
            &mut self.description,
            &mut self.sitename,
            &mut self.date,
            &mut self.categories,
            &mut self.tags,
            &mut self.fingerprint,
            &mut self.id,
            &mut self.license,
            &mut self.raw_text,
            &mut self.language,
            &mut self.image,
            &mut self.pagetype,
            &mut self.filedate,
        ]
    }
}
