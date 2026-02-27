use std::collections::HashMap;
use std::io::Cursor;

use rocket::fairing::{AdHoc, Fairing, Info, Kind};
use rocket::http::{Header, Method, Status, uri::Absolute};
use rocket::serde::{Deserialize, Serialize};
use rocket::{Orbit, Request, Response, Rocket};

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Cors {
    #[serde(default)]
    cors: HashMap<Absolute<'static>, Vec<Method>>,
}

impl std::fmt::Display for Cors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        for (host, methods) in &self.cors {
            write!(f, "{host}:")?;
            for method in methods {
                write!(f, " {method}")?;
            }
        }
        Ok(())
    }
}

impl Cors {
    pub fn fairing() -> impl Fairing {
        AdHoc::try_on_ignite("CORS Configuration", |rocket| async {
            match rocket.figment().extract::<Cors>() {
                Ok(cors) => Ok(rocket.attach(cors)),
                Err(e) => {
                    let kind = rocket::error::ErrorKind::Config(e);
                    rocket::Error::from(kind).pretty_print();
                    Err(rocket)
                }
            }
        })
    }
}

#[rocket::async_trait]
impl Fairing for Cors {
    fn info(&self) -> Info {
        Info {
            name: "CORS",
            kind: Kind::Liftoff | Kind::Response,
        }
    }

    async fn on_liftoff(&self, _rocket: &Rocket<Orbit>) {
        info!("{}", "CORS:");
        if self.cors.is_empty() {
            info_!("status: disabled");
        } else {
            info_!("status: enabled");
            info_!("{self}");
        }
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, resp: &mut Response<'r>) {
        let allowed_host_methods = req
            .headers()
            .get_one("Origin")
            .and_then(|origin| Absolute::parse(origin).ok())
            .and_then(|host| self.cors.get_key_value(&host))
            .filter(|(_, methods)| methods.contains(&req.method()));

        if let Some((host, methods)) = allowed_host_methods {
            const ALLOW_ORIGIN: &str = "Access-Control-Allow-Origin";
            const ALLOW_METHODS: &str = "Access-Control-Allow-Methods";
            const ALLOW_HEADERS: &str = "Access-Control-Allow-Headers";

            let allow_methods = methods
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(",");

            resp.set_header(Header::new(ALLOW_ORIGIN, host.to_string()));
            resp.set_header(Header::new(ALLOW_METHODS, allow_methods));
            resp.set_header(Header::new(ALLOW_HEADERS, "Content-Type"));

            if req.method() == Method::Options && resp.status() == Status::NotFound {
                resp.set_status(Status::Ok);
                resp.set_sized_body(0, Cursor::new(""));
            }
        }
    }
}
