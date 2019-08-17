use crate::templates;
use rich_crate::Readme;
use crate::Page;
use kitchen_sink::KitchenSink;
use render_readme::Renderer;
use rich_crate::RichCrateVersion;

pub struct InstallPage<'a> {
    pub is_dev: bool,
    pub is_build: bool,
    pub ver: &'a RichCrateVersion,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
}

impl<'a> InstallPage<'a> {
    pub fn new(ver: &'a RichCrateVersion, kitchen_sink: &'a KitchenSink, markup: &'a Renderer) -> Self {
        let (is_build, is_dev) = kitchen_sink.is_build_or_dev(ver.origin()).expect("deps");
        Self {
            is_build, is_dev,
            ver,
            kitchen_sink,
            markup,
        }
    }

    pub fn page(&self) -> Page {
        Page {
            title: self.page_title(),
            keywords: None,
            created: None,
            description: None,
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            alternate: None,
            alternate_type: None,
            canonical: None,
            noindex: true,
            search_meta: false,
            critical_css_data: None,
        }
    }

    /// docs.rs link, if available
    pub fn api_reference_url(&self) -> Option<String> {
        if self.kitchen_sink.has_docs_rs(self.ver.origin(), self.ver.short_name(), self.ver.version()) {
            Some(format!("https://docs.rs/{}", self.ver.short_name()))
        } else {
            None
        }
    }

    pub fn render_readme(&self, readme: &Readme) -> templates::Html<String> {
        let urls = match (readme.base_url.as_ref(), readme.base_image_url.as_ref()) {
            (Some(l), Some(i)) => Some((l.as_str(), i.as_str())),
            (Some(l), None) => Some((l.as_str(), l.as_str())),
            _ => None,
        };
        let (html, warnings) = self.markup.page(&readme.markup, urls, true, Some(self.ver.short_name()));
        if !warnings.is_empty() {
            eprintln!("{} readme: {:?}", self.ver.short_name(), warnings);
        }
        templates::Html(html)
    }

    pub fn page_title(&self) -> String {
        format!("How to install the {} crate", self.ver.short_name())
    }
}
