use crate::templates;
use crate::Page;
use kitchen_sink::KitchenSink;
use render_readme::Renderer;
use rich_crate::Readme;
use rich_crate::RichCrateVersion;

pub struct InstallPage<'a> {
    pub is_dev: bool,
    pub is_build: bool,
    pub ver: &'a RichCrateVersion,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
    api_reference_url: Option<String>,
}

impl<'a> InstallPage<'a> {
    pub async fn new(ver: &'a RichCrateVersion, kitchen_sink: &'a KitchenSink, markup: &'a Renderer) -> InstallPage<'a> {
        let (is_build, is_dev) = kitchen_sink.is_build_or_dev(ver.origin()).await.expect("deps");
        let api_reference_url = if kitchen_sink.has_docs_rs(ver.origin(), ver.short_name(), ver.version()).await {
            Some(format!("https://docs.rs/{}", ver.short_name()))
        } else {
            None
        };
        Self {
            is_build, is_dev,
            ver,
            kitchen_sink,
            markup,
            api_reference_url,
        }
    }

    pub fn page(&self) -> Page {
        Page {
            title: self.page_title(),
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            noindex: true,
            search_meta: false,
            ..Default::default()
        }
    }

    /// docs.rs link, if available
    pub fn api_reference_url(&self) -> Option<&str> {
        self.api_reference_url.as_deref()
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
