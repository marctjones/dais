#![allow(dead_code)]

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuthPolicy {
    PublicOnly,
    BearerSecretName(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IntegrationPolicy {
    pub auth: AuthPolicy,
    pub no_paywall_bypass: bool,
    pub private_reader_only: bool,
    pub excerpt_only: bool,
    pub attribution_required: bool,
    pub link_required: bool,
    pub max_poll_minutes: u16,
    pub terms_note: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceCandidate {
    pub canonical_url: String,
    pub kind: String,
    pub title_hint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExtractedSourceItem {
    pub title: String,
    pub canonical_url: String,
    pub external_id: Option<String>,
    pub author_or_org: Option<String>,
    pub published_at: Option<String>,
    pub excerpt: Option<String>,
    pub pdf_url: Option<String>,
    pub checksum: Option<String>,
    pub tags: Vec<String>,
    pub attribution: String,
    pub generated_summary: Option<String>,
}

pub trait SourceIntegration {
    fn name(&self) -> &'static str;
    fn policy(&self) -> &IntegrationPolicy;
    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>>;
    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem>;
}

pub trait EnrichmentProvider {
    fn name(&self) -> &'static str;
    fn enrich(&self, item: &ExtractedSourceItem) -> Result<EnrichmentResult>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnrichmentResult {
    pub summary: Option<String>,
    pub topics: Vec<String>,
    pub entities: Vec<String>,
    pub provenance: String,
}

pub fn default_public_policy() -> IntegrationPolicy {
    IntegrationPolicy {
        auth: AuthPolicy::PublicOnly,
        no_paywall_bypass: true,
        private_reader_only: true,
        excerpt_only: true,
        attribution_required: true,
        link_required: true,
        max_poll_minutes: 60,
        terms_note: None,
    }
}

#[derive(Clone, Debug)]
pub struct SitemapIntegration {
    policy: IntegrationPolicy,
}

impl Default for SitemapIntegration {
    fn default() -> Self {
        Self {
            policy: default_public_policy(),
        }
    }
}

impl SourceIntegration for SitemapIntegration {
    fn name(&self) -> &'static str {
        "sitemap"
    }

    fn policy(&self) -> &IntegrationPolicy {
        &self.policy
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        Ok(all_between(body, "<loc>", "</loc>")
            .into_iter()
            .map(|url| SourceCandidate {
                canonical_url: decode_entities(&url),
                kind: "html".to_string(),
                title_hint: None,
            })
            .collect())
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        HtmlPageIntegration::default().extract(body, url)
    }
}

#[derive(Clone, Debug)]
pub struct HtmlPageIntegration {
    policy: IntegrationPolicy,
}

impl Default for HtmlPageIntegration {
    fn default() -> Self {
        Self {
            policy: default_public_policy(),
        }
    }
}

impl SourceIntegration for HtmlPageIntegration {
    fn name(&self) -> &'static str {
        "html"
    }

    fn policy(&self) -> &IntegrationPolicy {
        &self.policy
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        Ok(link_hrefs(body)
            .into_iter()
            .map(|href| SourceCandidate {
                canonical_url: href,
                kind: "html".to_string(),
                title_hint: None,
            })
            .collect())
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        Ok(ExtractedSourceItem {
            title: meta(body, "og:title")
                .or_else(|| tag_text(body, "title"))
                .unwrap_or_else(|| "(untitled public page)".to_string()),
            canonical_url: canonical(body).unwrap_or_else(|| url.to_string()),
            external_id: None,
            author_or_org: meta(body, "author").or_else(|| meta(body, "article:author")),
            published_at: meta(body, "article:published_time").or_else(|| meta(body, "date")),
            excerpt: meta(body, "description").map(|value| excerpt(&value, 800)),
            pdf_url: first_pdf_link(body),
            checksum: None,
            tags: Vec::new(),
            attribution: url.to_string(),
            generated_summary: None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PdfMetadataIntegration {
    policy: IntegrationPolicy,
}

impl Default for PdfMetadataIntegration {
    fn default() -> Self {
        Self {
            policy: default_public_policy(),
        }
    }
}

impl SourceIntegration for PdfMetadataIntegration {
    fn name(&self) -> &'static str {
        "pdf-metadata"
    }

    fn policy(&self) -> &IntegrationPolicy {
        &self.policy
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        Ok(link_hrefs(body)
            .into_iter()
            .filter(|href| href.to_ascii_lowercase().ends_with(".pdf"))
            .map(|href| SourceCandidate {
                canonical_url: href,
                kind: "pdf".to_string(),
                title_hint: None,
            })
            .collect())
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        HtmlPageIntegration::default().extract(body, url)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScotusOpinionIntegration {
    html: HtmlPageIntegration,
}

impl SourceIntegration for ScotusOpinionIntegration {
    fn name(&self) -> &'static str {
        "scotus-opinions"
    }

    fn policy(&self) -> &IntegrationPolicy {
        self.html.policy()
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        PdfMetadataIntegration::default().discover(body)
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        let mut item = self.html.extract(body, url)?;
        item.external_id = meta(body, "docket").or_else(|| labeled_text(body, "Docket"));
        item.published_at = item
            .published_at
            .or_else(|| labeled_text(body, "Date Decided"));
        item.tags.push("legal-opinion".to_string());
        item.tags.push("scotus".to_string());
        item.generated_summary = item.excerpt.as_ref().map(|value| {
            format!(
                "Private generated summary candidate: {}",
                excerpt(value, 240)
            )
        });
        Ok(item)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InstitutionalReportIntegration {
    html: HtmlPageIntegration,
}

impl SourceIntegration for InstitutionalReportIntegration {
    fn name(&self) -> &'static str {
        "institutional-reports"
    }

    fn policy(&self) -> &IntegrationPolicy {
        self.html.policy()
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        PdfMetadataIntegration::default().discover(body)
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        let mut item = self.html.extract(body, url)?;
        item.author_or_org = item
            .author_or_org
            .or_else(|| meta(body, "publisher"))
            .or_else(|| meta(body, "organization"));
        item.tags.push("institutional-report".to_string());
        Ok(item)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AwardAnnouncementIntegration {
    html: HtmlPageIntegration,
}

impl SourceIntegration for AwardAnnouncementIntegration {
    fn name(&self) -> &'static str {
        "award-announcements"
    }

    fn policy(&self) -> &IntegrationPolicy {
        self.html.policy()
    }

    fn discover(&self, body: &str) -> Result<Vec<SourceCandidate>> {
        self.html.discover(body)
    }

    fn extract(&self, body: &str, url: &str) -> Result<ExtractedSourceItem> {
        let mut item = self.html.extract(body, url)?;
        item.external_id = meta(body, "award:category").or_else(|| labeled_text(body, "Category"));
        item.tags.push("award".to_string());
        if let Some(recipient) = labeled_text(body, "Recipient") {
            item.tags.push(format!("recipient:{recipient}"));
        }
        Ok(item)
    }
}

#[derive(Clone, Debug)]
pub struct MockEnrichmentProvider {
    pub summary: String,
}

impl EnrichmentProvider for MockEnrichmentProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn enrich(&self, item: &ExtractedSourceItem) -> Result<EnrichmentResult> {
        if item.canonical_url.is_empty() {
            return Err(anyhow!("enrichment requires canonical source URL"));
        }
        Ok(EnrichmentResult {
            summary: Some(self.summary.clone()),
            topics: item.tags.clone(),
            entities: item.author_or_org.iter().cloned().collect(),
            provenance: format!("generated by {} from {}", self.name(), item.canonical_url),
        })
    }
}

fn meta(body: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{name}\"");
    let property = format!("property=\"{name}\"");
    body.split("<meta")
        .find(|part| part.contains(&needle) || part.contains(&property))
        .and_then(|part| attr(part, "content"))
}

fn canonical(body: &str) -> Option<String> {
    body.split("<link")
        .find(|part| part.contains("rel=\"canonical\""))
        .and_then(|part| attr(part, "href"))
}

fn first_pdf_link(body: &str) -> Option<String> {
    link_hrefs(body)
        .into_iter()
        .find(|href| href.to_ascii_lowercase().ends_with(".pdf"))
}

fn link_hrefs(body: &str) -> Vec<String> {
    body.split("<a")
        .filter_map(|part| attr(part, "href"))
        .collect()
}

fn attr(fragment: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=\"");
    let start = fragment.find(&prefix)? + prefix.len();
    let rest = &fragment[start..];
    let end = rest.find('"')?;
    Some(decode_entities(&rest[..end]))
}

fn tag_text(body: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    Some(decode_entities(
        &body[body.find(&open)? + open.len()..body.find(&close)?],
    ))
}

fn labeled_text(body: &str, label: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.split_once(label))
        .map(|(_, value)| {
            value
                .trim_matches(|c: char| c == ':' || c.is_whitespace())
                .to_string()
        })
        .filter(|value| !value.is_empty())
}

fn all_between(body: &str, open: &str, close: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(close) else {
            break;
        };
        values.push(rest[..end].trim().to_string());
        rest = &rest[end + close.len()..];
    }
    values
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

fn excerpt(value: &str, max_chars: usize) -> String {
    value
        .replace(['\n', '\r'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_discovers_public_pages() {
        let xml = "<urlset><url><loc>https://example.gov/report</loc></url></urlset>";
        let items = SitemapIntegration::default().discover(xml).unwrap();
        assert_eq!(items[0].canonical_url, "https://example.gov/report");
    }

    #[test]
    fn html_extracts_canonical_metadata_and_pdf_link() {
        let html = r#"<html><head>
          <title>Annual Report</title>
          <link rel="canonical" href="https://example.org/report">
          <meta name="description" content="A public annual report excerpt.">
          <meta name="publisher" content="Example NGO">
        </head><body><a href="https://example.org/report.pdf">PDF</a></body></html>"#;
        let item = InstitutionalReportIntegration::default()
            .extract(html, "https://example.org/report")
            .unwrap();
        assert_eq!(item.title, "Annual Report");
        assert_eq!(
            item.pdf_url.as_deref(),
            Some("https://example.org/report.pdf")
        );
        assert!(item.tags.contains(&"institutional-report".to_string()));
    }

    #[test]
    fn scotus_extracts_docket_and_tags_summary_as_private_generated() {
        let html = r#"<html><head>
          <meta property="og:title" content="Example v. United States">
          <meta name="description" content="Slip opinion excerpt">
        </head><body>Docket: 25-100<br>Date Decided: 2026-06-11<a href="opinion.pdf">PDF</a></body></html>"#;
        let item = ScotusOpinionIntegration::default()
            .extract(html, "https://www.supremecourt.gov/opinions/example")
            .unwrap();
        assert_eq!(
            item.external_id.as_deref(),
            Some("25-100<br>Date Decided: 2026-06-11<a href=\"opinion.pdf\">PDF</a></body></html>")
        );
        assert!(item.tags.contains(&"scotus".to_string()));
        assert!(item
            .generated_summary
            .as_deref()
            .unwrap()
            .contains("Private generated"));
    }

    #[test]
    fn award_adapter_extracts_award_category_and_recipient_tag() {
        let html = r#"<html><head>
          <meta property="og:title" content="Prize Announcement">
          <meta name="award:category" content="Physics">
        </head><body>Recipient: Ada Example</body></html>"#;
        let item = AwardAnnouncementIntegration::default()
            .extract(html, "https://prize.example/2026")
            .unwrap();
        assert_eq!(item.external_id.as_deref(), Some("Physics"));
        assert!(item.tags.contains(&"award".to_string()));
        assert!(item
            .tags
            .contains(&"recipient:Ada Example</body></html>".to_string()));
    }

    #[test]
    fn enrichment_preserves_source_provenance() {
        let item = ExtractedSourceItem {
            title: "Public item".to_string(),
            canonical_url: "https://example.com/item".to_string(),
            external_id: None,
            author_or_org: Some("Example Org".to_string()),
            published_at: None,
            excerpt: None,
            pdf_url: None,
            checksum: None,
            tags: vec!["report".to_string()],
            attribution: "https://example.com/item".to_string(),
            generated_summary: None,
        };
        let enriched = MockEnrichmentProvider {
            summary: "Private summary".to_string(),
        }
        .enrich(&item)
        .unwrap();
        assert_eq!(enriched.summary.as_deref(), Some("Private summary"));
        assert!(enriched.provenance.contains("https://example.com/item"));
    }

    #[test]
    fn default_policy_rejects_paywall_bypass_shape() {
        let policy = default_public_policy();
        assert_eq!(policy.auth, AuthPolicy::PublicOnly);
        assert!(policy.no_paywall_bypass);
        assert!(policy.private_reader_only);
        assert!(policy.excerpt_only);
    }
}
