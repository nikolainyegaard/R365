//! Windows 365 / AVD workspace feed: ARM feed discovery, then one RADC XML feed per tenant.
//! Parsing is namespace-agnostic and attribute-case-insensitive because the service has varied
//! both. See docs/architecture.md.

use serde::{Deserialize, Serialize};

pub const DISCOVERY_URL: &str = "https://rdweb.wvd.microsoft.com/api/arm/feeddiscovery";
/// The web client's identity; the feed service answers other agents with HTML.
pub const USER_AGENT: &str = "com.microsoft.rdc.html/2.0.79.2 rdhtml-sdk/2.0.4";
pub const ACCEPT_DISCOVERY: &str = "application/x-msts-radc-discovery+xml,text/xml";
pub const ACCEPT_FEED: &str = "application/x-msts-radc+xml;radc_schema_version=2.0,text/xml";
pub const MAX_XML_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_RDP_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TENANT_FEEDS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantFeed {
    pub tenant_id: String,
    pub display_name: String,
    pub feed_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    Desktop,
    RemoteApp,
    Unknown,
}

impl ResourceKind {
    fn parse(s: Option<&str>) -> Self {
        match s.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("desktop") => Self::Desktop,
            Some("remoteapp") => Self::RemoteApp,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Desktop => "Cloud PC",
            Self::RemoteApp => "RemoteApp",
            Self::Unknown => "Resource",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub kind: ResourceKind,
    pub tenant_id: String,
    pub tenant_name: String,
    pub publisher: String,
    pub rdp_url: Option<String>,
    pub icon_url: Option<String>,
}

impl Resource {
    /// Stable identity across tenants; also the session key.
    pub fn key(&self) -> String {
        format!("{}:{}", self.tenant_id, self.id)
    }

    /// Safe for a file name.
    pub fn file_stem(&self) -> String {
        self.key().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' }).collect()
    }

    pub fn subtitle(&self) -> &str {
        if !self.publisher.trim().is_empty() {
            &self.publisher
        } else if !self.tenant_name.trim().is_empty() {
            &self.tenant_name
        } else {
            self.kind.label()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub tenant_id: String,
    pub tenant_name: String,
    pub resources: Vec<Resource>,
}

pub fn parse_discovery(xml: &str) -> crate::Result<Vec<TenantFeed>> {
    let doc = parse_doc(xml)?;
    let mut feeds = Vec::new();
    for (i, node) in doc.descendants().filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("TenantFeedURL")).enumerate() {
        let Some(url) = attr(node, &["FeedURL", "url", "href"]).and_then(allowed_url) else { continue };
        let tenant_id = attr(node, &["TenantId", "id"]).unwrap_or_else(|| format!("tenant-{}", i + 1));
        let display_name = attr(node, &["TenantDisplayName", "DisplayName", "name"]).unwrap_or_else(|| tenant_id.clone());
        feeds.push(TenantFeed { tenant_id, display_name, feed_url: url });
        if feeds.len() == MAX_TENANT_FEEDS {
            break;
        }
    }
    Ok(feeds)
}

pub fn parse_feed(xml: &str, tenant: &TenantFeed) -> crate::Result<Workspace> {
    let doc = parse_doc(xml)?;
    let mut resources = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("Resource")) {
        let publisher = node
            .ancestors()
            .find(|a| a.tag_name().name().eq_ignore_ascii_case("Publisher"))
            .and_then(|p| attr(p, &["Name", "Title"]))
            .unwrap_or_else(|| tenant.display_name.clone());
        let id = attr(node, &["ID"]).unwrap_or_else(|| format!("resource-{}", resources.len() + 1));
        let title = attr(node, &["Title", "Name"]).unwrap_or_else(|| id.clone());
        let rdp_url = child_url(node, &["ResourceFile"], &["URL", "href"]);
        let icon_url = child_url(node, &["Icon64", "Icon48", "Icon32", "Icon128", "Icon256", "Icon16", "Icon", "IconRaw"], &["FileURL", "URL", "href", "src"]);
        resources.push(Resource {
            id,
            title,
            kind: ResourceKind::parse(attr(node, &["Type", "ResourceType"]).as_deref()),
            tenant_id: tenant.tenant_id.clone(),
            tenant_name: tenant.display_name.clone(),
            publisher,
            rdp_url,
            icon_url,
        });
    }
    Ok(Workspace { tenant_id: tenant.tenant_id.clone(), tenant_name: tenant.display_name.clone(), resources })
}

fn parse_doc(xml: &str) -> crate::Result<roxmltree::Document<'_>> {
    if xml.len() > MAX_XML_BYTES {
        return Err("Feed document is larger than 10 MiB".into());
    }
    let start = xml.find('<').ok_or_else(|| "Feed response was not XML".to_string())?;
    let body = xml.get(start..).unwrap_or("");
    roxmltree::Document::parse(body).map_err(|e| format!("Feed XML could not be parsed: {e}"))
}

fn attr(node: roxmltree::Node<'_, '_>, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(v) = node.attributes().find(|a| a.name().eq_ignore_ascii_case(name)).map(|a| a.value().trim()).filter(|v| !v.is_empty()) {
            return Some(v.to_string());
        }
    }
    None
}

fn child_url(node: roxmltree::Node<'_, '_>, tags: &[&str], names: &[&str]) -> Option<String> {
    for tag in tags {
        for child in node.descendants().filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(tag)) {
            let value = attr(child, names).or_else(|| child.text().map(str::trim).filter(|t| !t.is_empty() && !t.contains('<')).map(str::to_string));
            if let Some(url) = value.and_then(allowed_url) {
                return Some(url);
            }
        }
    }
    None
}

/// Only https URLs on Microsoft's own hosts leave this module; everything else is dropped.
pub fn allowed_url(s: String) -> Option<String> {
    let rest = s.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, port)| if port == "443" { h } else { "" }).to_ascii_lowercase();
    is_microsoft_host(&host).then_some(s)
}

pub fn is_microsoft_host(host: &str) -> bool {
    const SUFFIXES: [&str; 11] = [
        "microsoft.com",
        "azure.com",
        "windows.net",
        "azureedge.net",
        "office.com",
        "microsoftonline.com",
        "msftauth.net",
        "trafficmanager.net",
        "azure.us",
        "azure.cn",
        "wvd.microsoft.com",
    ];
    !host.is_empty() && SUFFIXES.iter().any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISCOVERY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<FeedDiscoveryResult xmlns="http://schemas.microsoft.com/2008/11/msts/radc">
  <TenantFeedURL FeedURL="https://rdweb.wvd.microsoft.com/api/arm/feeddiscovery/tenant/t1" TenantId="t1" TenantDisplayName="Contoso" />
  <TenantFeedURL FeedURL="http://evil.example/feed" TenantId="t2" TenantDisplayName="Bad" />
</FeedDiscoveryResult>"#;

    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ResourceCollection xmlns="http://schemas.microsoft.com/2008/11/msts/radc">
  <Publisher Name="Contoso Enterprise">
    <Resources>
      <Resource ID="/subs/1/hp1" Title="Cloud PC 01" Type="Desktop">
        <ResourceFile URL="https://rdweb.wvd.microsoft.com/api/arm/rdp/cloudpc01.rdp" />
        <Icon32 FileURL="https://rdweb.wvd.microsoft.com/api/arm/icons/cloudpc01.png" />
      </Resource>
      <Resource ID="/subs/1/app1" Title="Word" Type="RemoteApp">
        <ResourceFile URL="https://attacker.example/x.rdp" />
      </Resource>
    </Resources>
  </Publisher>
</ResourceCollection>"#;

    #[test]
    fn discovery_keeps_only_https_microsoft() {
        let feeds = parse_discovery(DISCOVERY).unwrap();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].display_name, "Contoso");
    }

    #[test]
    fn feed_resources() {
        let tenant = TenantFeed { tenant_id: "t1".into(), display_name: "Contoso".into(), feed_url: "https://rdweb.wvd.microsoft.com/f".into() };
        let ws = parse_feed(FEED, &tenant).unwrap();
        assert_eq!(ws.resources.len(), 2);
        let pc = &ws.resources[0];
        assert_eq!(pc.kind, ResourceKind::Desktop);
        assert_eq!(pc.publisher, "Contoso Enterprise");
        assert!(pc.rdp_url.as_deref().unwrap().ends_with("cloudpc01.rdp"));
        assert!(pc.icon_url.is_some());
        assert_eq!(ws.resources[1].rdp_url, None);
        assert_eq!(pc.file_stem(), "t1__subs_1_hp1");
    }

    #[test]
    fn host_policy() {
        assert!(allowed_url("https://rdweb.wvd.microsoft.com/x".into()).is_some());
        assert!(allowed_url("https://rdweb.wvd.microsoft.com:443/x".into()).is_some());
        assert!(allowed_url("https://rdweb.wvd.microsoft.com:8443/x".into()).is_none());
        assert!(allowed_url("https://user@rdweb.wvd.microsoft.com/x".into()).is_none());
        assert!(allowed_url("https://notmicrosoft.com/x".into()).is_none());
        assert!(allowed_url("http://rdweb.wvd.microsoft.com/x".into()).is_none());
    }
}
