//! Go `macros.EndpointTemplateParams` and the `text/template` subset adapters use on endpoint
//! URLs: `{{.Field}}` substitution only (no conditionals or functions appear in any bidder's
//! endpoint).

/// Go `macros.EndpointTemplateParams`. Field names are the Go names, which the templates use.
#[derive(Debug, Clone, Default)]
pub struct EndpointTemplateParams {
    pub host: String,
    pub publisher_id: String,
    pub zone_id: String,
    pub source_id: String,
    pub account_id: String,
    pub ad_unit: String,
    pub media_type: String,
    pub gvl_id: String,
    pub page_id: String,
    pub supply_id: String,
    pub imp_id: String,
    pub ssp_id_lower: String,
    pub ssp_id: String,
    pub seat_id: String,
    pub token_id: String,
    pub partner_id: String,
    pub region: String,
    pub placement_id: String,
}

impl EndpointTemplateParams {
    fn get(&self, field: &str) -> Option<&str> {
        Some(match field {
            "Host" => &self.host,
            "PublisherID" => &self.publisher_id,
            "ZoneID" => &self.zone_id,
            "SourceId" => &self.source_id,
            "AccountID" => &self.account_id,
            "AdUnit" => &self.ad_unit,
            "MediaType" => &self.media_type,
            "GvlID" => &self.gvl_id,
            "PageID" => &self.page_id,
            "SupplyId" => &self.supply_id,
            "ImpID" => &self.imp_id,
            "SspId" => &self.ssp_id_lower,
            "SspID" => &self.ssp_id,
            "SeatID" => &self.seat_id,
            "TokenID" => &self.token_id,
            "PartnerId" => &self.partner_id,
            "Region" => &self.region,
            "PlacementID" => &self.placement_id,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Field(String),
}

/// A parsed endpoint template (Go `*template.Template`).
#[derive(Debug, Clone)]
pub struct EndpointTemplate(Vec<Part>);

impl EndpointTemplate {
    /// Go `template.New("endpointTemplate").Parse(endpoint)`. Only the failures adapters can
    /// hit are reported: an unclosed action, or an action that is not `{{.Field}}`. Go rejects
    /// `{{Malformed}}` here with `function "Malformed" not defined` (adapters whose `Builder`
    /// never parses the endpoint, like cointraffic, never see it).
    pub fn parse(endpoint: &str) -> Result<Self, String> {
        let mut parts = Vec::new();
        let mut rest = endpoint;
        while let Some(open) = rest.find("{{") {
            if open > 0 {
                parts.push(Part::Text(rest[..open].to_string()));
            }
            let after = &rest[open + 2..];
            let Some(close) = after.find("}}") else {
                return Err("template: endpointTemplate:1: unclosed action".to_string());
            };
            let action = after[..close].trim();
            if !action.starts_with('.') {
                return Err(format!(
                    "template: endpointTemplate:1: function \"{action}\" not defined"
                ));
            }
            parts.push(Part::Field(action.to_string()));
            rest = &after[close + 2..];
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_string()));
        }
        Ok(Self(parts))
    }

    /// Go `macros.ResolveMacros(template, params)`.
    pub fn resolve(&self, params: &EndpointTemplateParams) -> Result<String, String> {
        let mut out = String::new();
        for part in &self.0 {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Field(f) => match f.strip_prefix('.').and_then(|name| params.get(name)) {
                    Some(v) => out.push_str(v),
                    None => {
                        return Err(format!(
                            "template: endpointTemplate:1: function \"{f}\" not defined"
                        ))
                    }
                },
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_fields() {
        let t = EndpointTemplate::parse("https://{{.Host}}/p/{{.PublisherID}}").unwrap();
        let p = EndpointTemplateParams { host: "h.io".into(), publisher_id: "7".into(), ..Default::default() };
        assert_eq!(t.resolve(&p).unwrap(), "https://h.io/p/7");
    }

    #[test]
    fn malformed_fails_to_parse() {
        let err = EndpointTemplate::parse("{{Malformed}}").unwrap_err();
        assert_eq!(err, "template: endpointTemplate:1: function \"Malformed\" not defined");
    }

    #[test]
    fn unclosed_action_fails_to_parse() {
        assert!(EndpointTemplate::parse("http://x/{{.Host").is_err());
    }
}
