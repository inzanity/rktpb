use rocket::figment::providers::{Env, Format, Serialized, Toml};
use rocket::figment::value::magic::RelativePathBuf;
use rocket::figment::{Figment, Profile};
use rocket::serde::{Deserialize, Serialize, de};

use rocket::data::{ByteUnit, Limits, ToByteUnit};
use rocket::http::{Status, uri::Absolute};
use rocket::outcome::IntoOutcome;
use rocket::request::{FromRequest, Outcome};
use rocket::{Ignite, Request, Rocket, Sentinel};

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Config {
    pub id_length: usize,
    pub paste_limit: ByteUnit,
    pub server_url: Absolute<'static>,
    #[serde(deserialize_with = "directory")]
    #[serde(serialize_with = "RelativePathBuf::serialize_original")]
    pub upload_dir: RelativePathBuf,
}

impl Config {
    pub fn figment() -> Figment {
        #[cfg(debug_assertions)]
        const DEFAULT_PROFILE: &str = "debug";
        #[cfg(not(debug_assertions))]
        const DEFAULT_PROFILE: &str = "release";

        // This the base figment, without our `Config` defaults.
        let mut figment = Figment::new()
            .join(rocket::Config::default())
            .merge(Toml::file(Env::var_or("PASTE_CONFIG", "Paste.toml")).nested())
            .merge(Env::prefixed("PASTE_").profile(Profile::Global))
            .select(Profile::from_env_or("PASTE_PROFILE", DEFAULT_PROFILE));

        // Dynamically determine `server_url` default based on address/port.
        let default_server_url = match figment.extract() {
            Ok(config @ rocket::Config { address, port, .. }) => {
                let proto = if config.tls_enabled() {
                    "https"
                } else {
                    "http"
                };
                let url = format!("{proto}://{address}:{port}");
                Absolute::parse_owned(url).expect("default URL should be Absolute")
            }
            Err(_) => uri!("http://127.0.0.1:8017"),
        };

        // Now set the `Config` defaults.
        figment = figment.join(Serialized::defaults(Config {
            id_length: 3,
            paste_limit: 384.kibibytes(),
            server_url: default_server_url,
            upload_dir: "upload".into(),
        }));

        // Configure Rocket based on `Config` settings. If this fails now, it's
        // fine - it'll fail when attached too, so we won't miss out.
        if let Ok(config) = figment.extract::<Self>() {
            figment = figment
                .merge((rocket::Config::TEMP_DIR, config.upload_dir))
                .merge((
                    rocket::Config::LIMITS,
                    Limits::default()
                        .limit("form", config.paste_limit)
                        .limit("data-form", config.paste_limit)
                        .limit("file", config.paste_limit)
                        .limit("string", config.paste_limit)
                        .limit("bytes", config.paste_limit)
                        .limit("json", config.paste_limit)
                        .limit("msgpack", config.paste_limit)
                        .limit("paste", config.paste_limit),
                ));
        }

        figment
    }
}

impl Sentinel for Config {
    fn abort(rocket: &Rocket<Ignite>) -> bool {
        rocket.state::<Self>().is_none()
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r Config {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        req.rocket()
            .state::<Config>()
            .or_error((Status::InternalServerError, ()))
    }
}

fn directory<'de, D: de::Deserializer<'de>>(de: D) -> Result<RelativePathBuf, D::Error> {
    let path = RelativePathBuf::deserialize(de)?;
    let resolved = path.relative();

    if resolved.is_dir() {
        Ok(path)
    } else if !resolved.exists() {
        Err(de::Error::custom(format!(
            "Path {path} does not exist.",
            path = resolved.display()
        )))
    } else {
        Err(de::Error::custom(format!(
            "Path {path} is not a directory.",
            path = resolved.display(),
        )))
    }
}
