use super::{FtpConfig, FtpPool};
use crate::session::remote_fs::RemoteFs;
use crate::vault::model::FtpTlsMode;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use zeroize::Zeroizing;

async fn spawn_server_with_failed_retr_completion() -> u16 {
    let control = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = control.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (socket, _) = control.accept().await.unwrap();
        let (reader, mut writer) = socket.into_split();
        let mut reader = BufReader::new(reader);
        writer.write_all(b"220 test FTP ready\r\n").await.unwrap();

        let mut passive = None;
        let mut command = String::new();
        loop {
            command.clear();
            if reader.read_line(&mut command).await.unwrap() == 0 {
                break;
            }
            let verb = command.split_whitespace().next().unwrap_or("");
            match verb {
                "USER" => writer
                    .write_all(b"331 password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 logged in\r\n").await.unwrap(),
                "TYPE" => writer.write_all(b"200 binary mode\r\n").await.unwrap(),
                "PASV" => {
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let data_port = listener.local_addr().unwrap().port();
                    writer
                        .write_all(
                            format!(
                                "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                                data_port / 256,
                                data_port % 256
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                    passive = Some(listener);
                }
                "RETR" => {
                    writer
                        .write_all(b"150 opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = passive.take().unwrap().accept().await.unwrap();
                    data.write_all(b"payload").await.unwrap();
                    data.shutdown().await.unwrap();
                    drop(data);
                    writer
                        .write_all(b"451 transfer completion failed\r\n")
                        .await
                        .unwrap();
                }
                _ => panic!("unexpected FTP command: {command:?}"),
            }
        }
    });
    port
}

#[tokio::test]
async fn reader_reports_the_final_transfer_reply_before_eof() {
    let port = spawn_server_with_failed_retr_completion().await;
    let pool = FtpPool::new(
        FtpConfig {
            host: "127.0.0.1".into(),
            port,
            username: "anonymous".into(),
            password: Zeroizing::new(String::new()),
            tls: FtpTlsMode::None,
            passive: true,
        },
        2,
    );

    let mut reader = pool.open_read("/file", 0).await.unwrap();
    let mut bytes = Vec::new();
    let error = reader.read_to_end(&mut bytes).await.unwrap_err();

    assert_eq!(bytes, b"payload");
    assert!(error.to_string().contains("451"), "{error}");
}
