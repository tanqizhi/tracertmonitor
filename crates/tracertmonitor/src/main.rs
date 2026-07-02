use std::net::Ipv4Addr;

fn main() -> Result<(), tracertmonitor::server::ServerError> {
    let session = tracertmonitor::demo::demo_session();
    let server = tracertmonitor::server::CockpitServer::bind((Ipv4Addr::LOCALHOST, 0), session)?;
    println!("TracertMonitor cockpit: http://{}", server.local_addr()?);
    server.run()
}
