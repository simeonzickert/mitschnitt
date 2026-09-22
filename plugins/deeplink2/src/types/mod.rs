mod auth_callback;
mod billing_refresh;
mod integration_callback;
mod share_open;

pub use auth_callback::*;
pub use billing_refresh::*;
pub use integration_callback::*;
pub use share_open::*;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::str::FromStr;

// "mitschnitt" ist das EINZIGE Schema, das dieser Fork registriert (siehe
// "schemes" in tauri.conf.json und tauri.conf.staging.json). Es fehlte hier --
// ein Freigabe-Link auf dem eigenen Schema wurde also abgewiesen, waehrend
// fuenf fremde Schemata angenommen wurden, die wir gar nicht empfangen
// koennen. Die Altnamen bleiben als Ersatzpfad fuer bereits installierte
// Kopien stehen.
const SHARE_OPEN_PREFIXES: [&str; 7] = [
    "mitschnitt://share/open",
    "anarlog://share/open",
    "anarlog-staging://share/open",
    "anarlog-dev://share/open",
    "hyprnote://share/open",
    "hyprnote-staging://share/open",
    "hypr://share/open",
];
const MAX_SHARE_OPEN_URL_BYTES: usize = 512;

#[derive(Debug, Clone, serde::Serialize, specta::Type, tauri_specta::Event)]
pub struct DeepLinkEvent(pub DeepLink);

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "to", content = "search")]
pub enum DeepLink {
    #[serde(rename = "/auth/callback")]
    AuthCallback(AuthCallbackSearch),
    #[serde(rename = "/billing/refresh")]
    BillingRefresh(BillingRefreshSearch),
    #[serde(rename = "/integration/callback")]
    IntegrationCallback(IntegrationCallbackSearch),
}

pub(crate) enum IncomingDeepLink {
    Existing(DeepLink),
    ShareOpen(ShareOpenRequest),
}

impl FromStr for IncomingDeepLink {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let candidate = s.trim_matches(|character: char| character.is_ascii_whitespace());
        if candidate.len() > MAX_SHARE_OPEN_URL_BYTES
            && SHARE_OPEN_PREFIXES.iter().any(|expected| {
                candidate
                    .get(..expected.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected))
            })
        {
            return Err(crate::Error::InvalidShareOpen);
        }

        let parsed = url::Url::parse(candidate)?;
        let host = parsed.host_str().unwrap_or("");
        let path = parsed.path().trim_start_matches('/');
        let full_path = if path.is_empty() {
            host.to_string()
        } else {
            format!("{host}/{path}")
        };

        if full_path == "share/open" {
            return ShareOpenRequest::parse(&parsed).map(Self::ShareOpen);
        }

        DeepLink::from_str(candidate).map(Self::Existing)
    }
}

#[cfg(test)]
mod incoming_tests {
    use super::*;

    // Wache gegen genau den Zustand, der hier vorlag: das einzige registrierte
    // Schema fehlte in der Annahmeliste, ein eigener Freigabe-Link lief also
    // ins Leere, ohne dass irgendwo ein Fehler sichtbar wurde.
    #[test]
    fn accepts_the_scheme_this_fork_actually_registers() {
        assert!(
            SHARE_OPEN_PREFIXES.contains(&"mitschnitt://share/open"),
            "das eigene Schema fehlt in SHARE_OPEN_PREFIXES"
        );
    }

    // Grok-Review 02.09.2026 (Befund 5): die Altlinks des Originals muessen
    // am EINTRITTSPUNKT weiter ankommen -- eine bereits installierte Kopie
    // hat "anarlog://" beim System registriert, und der Fork liest genau
    // diese Links. Gleichzeitig muss das eigene Schema alle Wege gehen:
    // vor diesem Test nahm `ShareOpenRequest::parse` "mitschnitt" nicht an,
    // obwohl SHARE_OPEN_PREFIXES es fuehrte -- der Waechter oben prueft die
    // Liste, nicht den Parser.
    #[test]
    fn accepts_old_and_own_schemes_at_the_entry_point() {
        for scheme in ["anarlog", "anarlog-dev", "hyprnote", "mitschnitt"] {
            assert!(
                matches!(
                    IncomingDeepLink::from_str(&format!(
                        "{scheme}://auth/callback?code=ac_nf5hq&state=s1"
                    )),
                    Ok(IncomingDeepLink::Existing(DeepLink::AuthCallback(_)))
                ),
                "{scheme}://auth/callback wird nicht mehr angenommen"
            );
            assert!(
                matches!(
                    IncomingDeepLink::from_str(&format!(
                        "{scheme}://share/open?mode=handoff&request_id=ba5ca57a-8f88-44e8-ab92-f9e10c89425c"
                    )),
                    Ok(IncomingDeepLink::ShareOpen(
                        ShareOpenRequest::Handoff { .. }
                    ))
                ),
                "{scheme}://share/open wird nicht angenommen"
            );
        }
    }

    #[test]
    fn rejects_oversized_share_open_before_url_parsing() {
        for prefix in SHARE_OPEN_PREFIXES {
            let value = format!("{prefix}?{}", "unknown=x&".repeat(64));
            assert!(value.len() > MAX_SHARE_OPEN_URL_BYTES);
            assert!(matches!(
                IncomingDeepLink::from_str(&value),
                Err(crate::Error::InvalidShareOpen)
            ));
            assert!(matches!(
                IncomingDeepLink::from_str(&format!(" \n{value}")),
                Err(crate::Error::InvalidShareOpen)
            ));
        }
    }
}

impl DeepLink {
    pub fn path(&self) -> &'static str {
        match self {
            DeepLink::AuthCallback(_) => "/auth/callback",
            DeepLink::BillingRefresh(_) => "/billing/refresh",
            DeepLink::IntegrationCallback(_) => "/integration/callback",
        }
    }
}

impl FromStr for DeepLink {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parsed = url::Url::parse(s)?;

        let host = parsed.host_str().unwrap_or("");
        let path = parsed.path().trim_start_matches('/');
        let full_path = if path.is_empty() {
            host.to_string()
        } else {
            format!("{}/{}", host, path)
        };

        let query = parsed.query().unwrap_or("");

        match full_path.as_str() {
            "auth/callback" => Ok(DeepLink::AuthCallback(serde_qs::from_str(query)?)),
            "billing/refresh" => Ok(DeepLink::BillingRefresh(serde_qs::from_str(query)?)),
            "integration/callback" => Ok(DeepLink::IntegrationCallback(serde_qs::from_str(query)?)),
            _ => Err(crate::Error::UnknownPath(full_path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Der Demo-Rueckruf ging am 02.09.2026 mit dem Demo-Link (ISA N5): die
    // Webseite des Originals bekam einen Localhost-Rueckruf auf diesen Pfad,
    // um unsere Aufnahme zu beenden. Nichts hoert mehr darauf, also nimmt
    // der Parser ihn auch nicht mehr an -- auf keinem Schema.
    #[test]
    fn rejects_the_removed_onboarding_demo_completion() {
        for scheme in ["mitschnitt", "anarlog"] {
            assert!(
                matches!(
                    DeepLink::from_str(&format!("{scheme}://onboarding-demo/complete")),
                    Err(crate::Error::UnknownPath(_))
                ),
                "{scheme}://onboarding-demo/complete wird noch angenommen"
            );
        }
    }

    #[test]
    fn parses_chatgpt_loopback_authorization_code() {
        let DeepLink::AuthCallback(search) =
            DeepLink::from_str("local://auth/callback?code=codex-code&state=s1&scope=openid")
                .unwrap()
        else {
            panic!("expected auth callback");
        };

        assert!(search.access_token.is_empty());
        assert!(search.refresh_token.is_empty());
        assert_eq!(search.code.as_deref(), Some("codex-code"));
        assert_eq!(search.state.as_deref(), Some("s1"));
    }

    #[test]
    fn parses_subscription_auth_custom_scheme_deeplink() {
        let DeepLink::AuthCallback(search) = DeepLink::from_str(
            "anarlog://auth/callback?code=ac_nf5hq&state=xYc5ZmNlqtWTu3BIbfbVQg",
        )
        .unwrap() else {
            panic!("expected auth callback");
        };

        assert!(search.access_token.is_empty());
        assert!(search.refresh_token.is_empty());
        assert_eq!(search.code.as_deref(), Some("ac_nf5hq"));
        assert_eq!(search.state.as_deref(), Some("xYc5ZmNlqtWTu3BIbfbVQg"));
    }
}
