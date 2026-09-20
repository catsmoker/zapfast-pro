//! Local-only call network diagnostics.
//!
//! ZapFast has no voice/video call stack (call messages are shown as
//! unsupported), so there is no active call, no STUN/TURN session, and no
//! peer endpoint to inspect. This module reports exactly that, plus the one
//! thing the device can observe by itself: the local address its own routing
//! table would use. It performs no network I/O and contacts no external
//! service.
//!
//! GeoIP, ASN, and public-IP lookups are intentionally unimplemented: they
//! need an external database or API, which would send user data to a third
//! party. Any future provider must be explicit opt-in, and its results must
//! always be labelled approximate: an IP address never identifies an exact
//! physical location.

/// Local outbound address without sending any traffic: UDP `connect()` only
/// consults the local routing table. 192.0.2.1 is TEST-NET-1 (RFC 5737), so
/// no real peer is ever involved, not even in theory.
pub fn local_addresses() -> Vec<String> {
    let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    if socket.connect("192.0.2.1:9").is_err() {
        return Vec::new();
    }
    socket
        .local_addr()
        .map(|addr| vec![addr.ip().to_string()])
        .unwrap_or_default()
}

/// A diagnostics row value. Anything the client cannot observe is an
/// explicit state, never a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticValue {
    Text(String),
    Unknown,
    Unavailable(&'static str),
    Relayed,
}

impl DiagnosticValue {
    pub fn label(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Unknown => "Unknown",
            Self::Unavailable(_) => "Unavailable",
            Self::Relayed => "Relayed",
        }
    }

    pub fn detail(&self) -> Option<&'static str> {
        match self {
            Self::Text(_) | Self::Unknown | Self::Relayed => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

/// Point-in-time snapshot rendered by the diagnostics panel.
#[derive(Clone, Debug)]
pub struct CallDiagnostics {
    pub call: DiagnosticValue,
    pub local: DiagnosticValue,
    pub connection_type: DiagnosticValue,
    pub public_ip: DiagnosticValue,
    pub asn_isp: DiagnosticValue,
    pub geo: DiagnosticValue,
    pub stun: DiagnosticValue,
    pub turn: DiagnosticValue,
    pub remote_peer: DiagnosticValue,
    pub updated_at: i64,
}

impl CallDiagnostics {
    pub fn snapshot() -> Self {
        let local = match local_addresses().first().cloned() {
            Some(addr) => DiagnosticValue::Text(addr),
            None => DiagnosticValue::Unavailable("No local route could be determined."),
        };
        Self {
            call: DiagnosticValue::Unavailable(
                "Voice and video calls are not supported in ZapFast yet.",
            ),
            local,
            connection_type: DiagnosticValue::Unknown,
            public_ip: DiagnosticValue::Unavailable(
                "Determining it would query an external service, which ZapFast does not do.",
            ),
            asn_isp: DiagnosticValue::Unavailable(
                "Determining it would query an external service, which ZapFast does not do.",
            ),
            geo: DiagnosticValue::Unavailable(
                "No GeoIP provider is configured. An IP address only ever indicates an approximate network location, never an exact address.",
            ),
            stun: DiagnosticValue::Unavailable("No call means no STUN session."),
            turn: DiagnosticValue::Unavailable("No call means no TURN session."),
            remote_peer: DiagnosticValue::Relayed,
            updated_at: crate::util::now(),
        }
    }

    /// Peer endpoint explanation. Never substitute relay infrastructure for
    /// either party's location or identity.
    pub fn remote_peer_detail() -> &'static str {
        "Peer IP unavailable: the call is being relayed, or there is no call. Relay server locations describe infrastructure, not people."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_never_claims_a_call_or_a_peer() {
        let diagnostics = CallDiagnostics::snapshot();
        assert_eq!(diagnostics.call.label(), "Unavailable");
        assert_eq!(diagnostics.remote_peer.label(), "Relayed");
        assert_eq!(diagnostics.stun.label(), "Unavailable");
        assert_eq!(diagnostics.turn.label(), "Unavailable");
        assert_eq!(diagnostics.public_ip.label(), "Unavailable");
        assert_eq!(diagnostics.asn_isp.label(), "Unavailable");
        assert_eq!(diagnostics.geo.label(), "Unavailable");
        assert_eq!(diagnostics.connection_type.label(), "Unknown");
    }

    #[test]
    fn relay_wording_never_substitutes_infrastructure_for_people() {
        let detail = CallDiagnostics::remote_peer_detail();
        assert!(detail.contains("Peer IP unavailable"));
        assert!(detail.contains("relayed"));
    }

    #[test]
    fn local_probing_never_panics_and_stays_textual() {
        // Sandboxes may have no route; both outcomes are honest states.
        let addrs = local_addresses();
        assert!(addrs.len() <= 1);
        for addr in addrs {
            assert!(!addr.is_empty());
        }
        let diagnostics = CallDiagnostics::snapshot();
        match &diagnostics.local {
            DiagnosticValue::Text(addr) => assert!(!addr.is_empty()),
            DiagnosticValue::Unavailable(_) => {}
            other => panic!("unexpected local state: {other:?}"),
        }
    }

    #[test]
    fn labels_stay_free_of_em_dashes() {
        // User-facing writing uses commas, colons, parentheses, or full stops.
        for value in [
            DiagnosticValue::Text("x".into()),
            DiagnosticValue::Unknown,
            DiagnosticValue::Unavailable("reason"),
            DiagnosticValue::Relayed,
        ] {
            assert!(!value.label().contains('\u{2014}'));
            assert!(!value.label().contains('\u{2013}'));
        }
        let detail = CallDiagnostics::remote_peer_detail();
        assert!(!detail.contains('\u{2014}'));
        assert!(!detail.contains('\u{2013}'));
    }
}
