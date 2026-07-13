use crate::model::{GeoIpConfig, GeoIpInfo, GeoIpLookupStatus};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeoIpLookupError {
    ProviderUnavailable,
    InvalidResponse,
}

pub trait OnlineGeoIpProvider {
    fn lookup(
        &mut self,
        ip: IpAddr,
        config: &GeoIpConfig,
    ) -> Result<Option<GeoIpInfo>, GeoIpLookupError>;
}

pub trait LocalGeoIpProvider {
    fn lookup(&mut self, ip: IpAddr, config: &GeoIpConfig) -> Option<GeoIpInfo>;
}

pub trait GeoIpResolver {
    fn resolve(&mut self, ip: IpAddr, config: &GeoIpConfig) -> GeoIpInfo;
}

#[derive(Debug, Default)]
pub struct DefaultGeoIpResolver {
    online: DisabledOnlineGeoIpProvider,
    local: EmptyLocalGeoIpProvider,
}

impl GeoIpResolver for DefaultGeoIpResolver {
    fn resolve(&mut self, ip: IpAddr, config: &GeoIpConfig) -> GeoIpInfo {
        resolve_with_providers(ip, config, &mut self.online, &mut self.local)
    }
}

#[derive(Debug, Default)]
struct DisabledOnlineGeoIpProvider;

impl OnlineGeoIpProvider for DisabledOnlineGeoIpProvider {
    fn lookup(
        &mut self,
        _ip: IpAddr,
        _config: &GeoIpConfig,
    ) -> Result<Option<GeoIpInfo>, GeoIpLookupError> {
        Err(GeoIpLookupError::ProviderUnavailable)
    }
}

#[derive(Debug, Default)]
struct EmptyLocalGeoIpProvider;

impl LocalGeoIpProvider for EmptyLocalGeoIpProvider {
    fn lookup(&mut self, _ip: IpAddr, _config: &GeoIpConfig) -> Option<GeoIpInfo> {
        None
    }
}

pub fn resolve_with_providers(
    ip: IpAddr,
    config: &GeoIpConfig,
    online: &mut impl OnlineGeoIpProvider,
    local: &mut impl LocalGeoIpProvider,
) -> GeoIpInfo {
    if config.skip_private_or_reserved && is_private_or_reserved(ip) {
        return status_only(GeoIpLookupStatus::SkippedPrivateOrReserved);
    }

    if config.online_enabled {
        if let Ok(Some(info)) = online.lookup(ip, config) {
            return with_status(info, GeoIpLookupStatus::OnlineHit);
        }
        if let Some(info) = local.lookup(ip, config) {
            return with_status(info, GeoIpLookupStatus::OnlineFailedLocalHit);
        }
        return status_only(GeoIpLookupStatus::Unknown);
    }

    if let Some(info) = local.lookup(ip, config) {
        return with_status(info, GeoIpLookupStatus::LocalHit);
    }

    status_only(GeoIpLookupStatus::Unknown)
}

fn with_status(mut info: GeoIpInfo, status: GeoIpLookupStatus) -> GeoIpInfo {
    info.status = status;
    info
}

fn status_only(status: GeoIpLookupStatus) -> GeoIpInfo {
    GeoIpInfo {
        country_or_region: None,
        province: None,
        city: None,
        carrier_or_asn: None,
        source: None,
        status,
    }
}

fn is_private_or_reserved(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => is_ipv4_private_or_reserved(addr),
        IpAddr::V6(addr) => is_ipv6_private_or_reserved(addr),
    }
}

fn is_ipv4_private_or_reserved(addr: Ipv4Addr) -> bool {
    let [a, b, c, _] = addr.octets();
    addr.is_private()
        || addr.is_loopback()
        || addr.is_link_local()
        || addr.is_multicast()
        || addr.is_unspecified()
        || addr.is_broadcast()
        || a == 0
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 240
}

fn is_ipv6_private_or_reserved(addr: Ipv6Addr) -> bool {
    let segments = addr.segments();
    addr.is_loopback()
        || addr.is_unspecified()
        || addr.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GeoIpConfig, GeoIpInfo, GeoIpLookupStatus};
    use std::net::{IpAddr, Ipv4Addr};

    struct RecordingOnlineProvider {
        calls: usize,
        result: Result<Option<GeoIpInfo>, GeoIpLookupError>,
    }

    impl OnlineGeoIpProvider for RecordingOnlineProvider {
        fn lookup(
            &mut self,
            _ip: IpAddr,
            _config: &GeoIpConfig,
        ) -> Result<Option<GeoIpInfo>, GeoIpLookupError> {
            self.calls += 1;
            self.result.clone()
        }
    }

    struct RecordingLocalProvider {
        calls: usize,
        result: Option<GeoIpInfo>,
    }

    impl LocalGeoIpProvider for RecordingLocalProvider {
        fn lookup(&mut self, _ip: IpAddr, _config: &GeoIpConfig) -> Option<GeoIpInfo> {
            self.calls += 1;
            self.result.clone()
        }
    }

    #[test]
    fn skips_private_or_reserved_addresses() {
        let config = GeoIpConfig::default();
        let mut online = RecordingOnlineProvider {
            calls: 0,
            result: Ok(Some(local_info())),
        };
        let mut local = RecordingLocalProvider {
            calls: 0,
            result: Some(local_info()),
        };

        let result = resolve_with_providers(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            &config,
            &mut online,
            &mut local,
        );

        assert_eq!(GeoIpLookupStatus::SkippedPrivateOrReserved, result.status);
        assert_eq!(0, online.calls);
        assert_eq!(0, local.calls);
    }

    #[test]
    fn online_failure_falls_back_to_local_hit() {
        let config = GeoIpConfig {
            online_enabled: true,
            skip_private_or_reserved: false,
            ..GeoIpConfig::default()
        };
        let mut online = RecordingOnlineProvider {
            calls: 0,
            result: Err(GeoIpLookupError::ProviderUnavailable),
        };
        let mut local = RecordingLocalProvider {
            calls: 0,
            result: Some(local_info()),
        };

        let result = resolve_with_providers(
            IpAddr::V4(Ipv4Addr::new(14, 215, 177, 39)),
            &config,
            &mut online,
            &mut local,
        );

        assert_eq!(GeoIpLookupStatus::OnlineFailedLocalHit, result.status);
        assert_eq!(Some("广东省"), result.province.as_deref());
        assert_eq!(Some("广州市"), result.city.as_deref());
        assert_eq!(Some("中国电信 AS4134"), result.carrier_or_asn.as_deref());
        assert_eq!(1, online.calls);
        assert_eq!(1, local.calls);
    }

    fn local_info() -> GeoIpInfo {
        GeoIpInfo {
            country_or_region: Some("中国".to_string()),
            province: Some("广东省".to_string()),
            city: Some("广州市".to_string()),
            carrier_or_asn: Some("中国电信 AS4134".to_string()),
            source: Some("local-fixture".to_string()),
            status: GeoIpLookupStatus::LocalHit,
        }
    }
}
