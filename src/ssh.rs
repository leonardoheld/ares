use async_trait::async_trait;
use russh::client;
use russh::{ChannelMsg, Disconnect, Preferred};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub struct Client {}

#[async_trait]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct Session {
    session: client::Handle<Client>,
}

impl Session {
    pub async fn connect<A: std::net::ToSocketAddrs + tokio::net::ToSocketAddrs>(
        username: String,
        password: String,
        addrs: A,
    ) -> Result<Self, russh::Error> {
        let config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(5)),
            preferred: Preferred {
                kex: Cow::Owned(vec![russh::kex::DH_G14_SHA256]),
                ..Default::default()
            },
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = Client {};

        let mut session = client::connect(config, addrs, sh).await?;
        let auth_res = session.authenticate_password(username, password).await?;

        if !auth_res {
            return Err(russh::Error::NotAuthenticated);
        }
        Ok(Self { session })
    }

    pub async fn call(&mut self, command: &str) -> Result<u32, russh::Error> {
        let mut channel = self.session.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut code = None;
        let mut stdout = tokio::io::stdout();

        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    code = Some(exit_status);
                }
                _ => {}
            }
        }
        Ok(code.expect("program did not exit cleanly"))
    }

    pub async fn close(&mut self) -> Result<(), russh::Error> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}
