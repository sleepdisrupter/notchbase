//! Alerts: shows alerts that other apps on this computer post to a local webhook.

mod alert;
mod config;
mod http;
mod server;
mod webhook;

use notchbase_sdk::prelude::*;

use config::Config;
use server::Server;
use webhook::Action;

pub const ID: &str = "alerts";

/// `generation` goes up on every restart, so a late failure from an old listener is ignored.
pub struct Alerts {
    config: Config,
    server: Option<Server>,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Unreachable { generation: u64, port: u16 },
    Webhook(Action),
    Configured(Settings),
}

impl Tool for Alerts {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Config::declare(
            Manifest::new(ID, "Alerts")
                .background()
                .description("Show alerts sent to a local webhook")
                .version(env!("CARGO_PKG_VERSION")),
        )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let mut alerts = Self { config: Config::from(context.settings()), server: None, generation: 0 };
        let command = alerts.restart();
        (alerts, command)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Unreachable { generation, port } if generation == self.generation => {
                self.server = None;
                Command::notify(format!("Alerts couldn't listen on port {port}"))
            }
            Msg::Unreachable { .. } => Command::none(),
            Msg::Webhook(Action::Show(alert)) => Command::notice(alert.notice()),
            Msg::Webhook(Action::Dismiss) => Command::dismiss_notice(),
            Msg::Configured(settings) => {
                let config = Config::from(&settings);
                if config == self.config {
                    return Command::none();
                }
                self.config = config;
                self.restart()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::settings(Msg::Configured)
    }
}

impl Alerts {
    fn restart(&mut self) -> Command<Msg> {
        self.server = None;
        self.generation += 1;
        if !self.config.enabled {
            return Command::none();
        }
        let (server, command) = server::start(self.config.port, self.config.token.clone(), self.generation);
        self.server = Some(server);
        command
    }
}

notchbase_sdk::export!(Alerts);

#[cfg(test)]
mod tests;
