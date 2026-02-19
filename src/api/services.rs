use lib_conxian_core::gateway::{BisqService, RGBService, ConxianService};
use serde::Serialize;

#[derive(Serialize)]
pub struct MultiProtocolStatus {
    pub services: Vec<lib_conxian_core::gateway::ServiceStatus>,
}

pub fn get_all_services_status() -> MultiProtocolStatus {
    let bisq = BisqService;
    let rgb = RGBService;

    MultiProtocolStatus {
        services: vec![
            bisq.status(),
            rgb.status(),
        ],
    }
}
