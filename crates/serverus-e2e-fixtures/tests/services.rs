use serverus_e2e_fixtures::ftp::FtpServer;
use serverus_e2e_fixtures::s3::S3Server;
use serverus_e2e_fixtures::workspace::FixtureWorkspace;

#[tokio::test]
async fn ftp_and_s3_control_ports_accept_connections() {
    let workspace = FixtureWorkspace::create().unwrap();
    let ftp = FtpServer::start(&workspace.paths().ftp_root).await.unwrap();
    let s3 = S3Server::start(&workspace.paths().s3_root).await.unwrap();

    assert_ne!(ftp.port(), 0);
    assert_ne!(s3.port(), 0);
    tokio::net::TcpStream::connect(("127.0.0.1", ftp.port()))
        .await
        .unwrap();
    tokio::net::TcpStream::connect(("127.0.0.1", s3.port()))
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn ssh_fixture_reports_a_reachable_fresh_key_server() {
    use serverus_e2e_fixtures::ssh::SshServer;

    let workspace = FixtureWorkspace::create().unwrap();
    let ssh = SshServer::start(workspace.paths()).await.unwrap();
    let manifest = ssh.manifest();

    assert!(manifest.available);
    assert!(manifest.key_path.as_ref().unwrap().is_file());
    tokio::net::TcpStream::connect(("127.0.0.1", manifest.port.unwrap()))
        .await
        .unwrap();
}
