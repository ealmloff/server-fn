use std::fmt::Debug;

#[doc(hidden)]
pub use ciborium;
#[cfg(feature = "server")]
#[doc(hidden)]
pub use inventory;
use once_cell::sync::OnceCell;
#[doc(hidden)]
pub use xxhash_rust;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use crate::axum::*;

static ROOT_URL: OnceCell<&'static str> = OnceCell::new();

pub fn set_root_url(url: &'static str) {
    ROOT_URL.set(url).unwrap();
}

#[cfg(all(feature = "client", not(target_arch = "wasm32")))]
fn get_root_url() -> &'static str {
    ROOT_URL
        .get()
        .expect("Call set_root_url before calling a server function.")
}

#[cfg(all(feature = "client", target_arch = "wasm32"))]
fn get_root_url() -> &'static str {
    use once_cell::sync::Lazy;
    static BACKUP_ROOT_URL: Lazy<&'static str> = Lazy::new(|| {
        Box::leak(
            window()
                .expect("expected window")
                .location()
                .href()
                .expect("expected href")
                .trim_end_matches('/')
                .to_string()
                .into_boxed_str(),
        )
    });
    use web_sys::window;
    ROOT_URL.get().copied().unwrap_or_else(|| *BACKUP_ROOT_URL)
}

#[cfg(feature = "server")]
pub fn server_fns() -> impl Iterator<Item = &'static ServerFn> {
    inventory::iter::<ServerFn>()
}

#[macro_export]
macro_rules! maybe_path {
    ($path:literal) => {
        $path
    };
    () => {
        "/api"
    };
}

#[macro_export]
macro_rules! server_fn {
    ($(@$path:literal)? $vis:vis async fn $name:ident($( $args:ident : $t:ty ),* $(,)?) $(-> Result<$ret:ty, RemoteCallError>)? { $($body:tt)* }) => {
        #[cfg(any(feature = "server", doc))]
        $vis async fn $name($($args : $t),*) $(-> Result<$ret, $crate::RemoteCallError>)? {
            const ID: u64 = $crate::xxhash_rust::const_xxh64::xxh64(concat!(env!("CARGO_MANIFEST_DIR"), ":", file!(), ":", line!(), ":", column!()).as_bytes(), 0);
            pub fn from_to_serde(input: &[u8]) -> $crate::SerdeFunctionWrapperReturn {
                let deserialized = $crate::ciborium::de::from_reader(input);
                Box::pin(async move {
                    let ($($args),*) = deserialized?;
                    let mut out: Vec<u8> = Vec::new();
                    $crate::ciborium::ser::into_writer(&inner($($args),*).await?, &mut out)?;
                    Ok(out)
                })
            }

            $crate::inventory::submit! {
                $crate::ServerFn {
                    id: ID,
                    fn_name: stringify!($name),
                    path: $crate::maybe_path!($($path)?),
                    func: from_to_serde,
                }
            }

            async fn inner($($args : $t),*) $(-> Result<$ret, $crate::RemoteCallError>)? {
                $($body)*
            }

            inner($($args),*).await
        }

        #[cfg(feature = "client")]
        $vis async fn $name($($args : $t),*) $(-> Result<$ret, $crate::RemoteCallError>)? {
            const ID: u64 = $crate::xxhash_rust::const_xxh64::xxh64(concat!(env!("CARGO_MANIFEST_DIR"), ":", file!(), ":", line!(), ":", column!()).as_bytes(), 0);
            $crate::fetch(($($args),*), ID, stringify!($name), $crate::maybe_path!($($path)?)).await
        }
    };
}

#[cfg(feature = "client")]
pub async fn fetch<I: serde::ser::Serialize, R: serde::de::DeserializeOwned>(
    data: I,
    id: u64,
    fn_name: &str,
    path: &str,
) -> Result<R, RemoteCallError> {
    let client = reqwest::Client::new();
    let root = get_root_url();
    let mut serialized: Vec<u8> = Vec::new();
    ciborium::ser::into_writer(&data, &mut serialized)?;
    let res = client
        .post(format!("{root}{path}/{fn_name}{id}"))
        .header("Content-Type", "application/cbor")
        .body(serialized)
        .send()
        .await?;
    let bytes = res.bytes().await?;
    let deserialized = ciborium::de::from_reader(&*bytes)?;
    Ok(deserialized)
}

#[cfg(feature = "server")]
inventory::collect!(ServerFn);

pub struct ServerFn {
    pub id: u64,
    pub fn_name: &'static str,
    pub path: &'static str,
    pub func: SerdeFunctionWrapper,
}

impl ServerFn {
    #[cfg(feature = "server")]
    fn route(&self) -> String {
        let Self {
            id,
            fn_name,
            path,
            func: _,
        } = self;
        format!("{path}/{fn_name}{id}",)
    }
}

impl Debug for ServerFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerFn")
            .field("fn_name", &self.fn_name)
            .field("id", &self.id)
            .field("path", &self.path)
            .finish()
    }
}

pub type SerdeFunctionWrapper = fn(&[u8]) -> SerdeFunctionWrapperReturn;

pub type SerdeFunctionWrapperReturn =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, RemoteCallError>> + Send>>;

#[derive(Debug)]
pub enum RemoteCallError {
    CiboriumSerilization(ciborium::ser::Error<<&'static [u8] as ciborium_io::Read>::Error>),
    CiboriumDeserilization(ciborium::de::Error<<Vec<u8> as ciborium_io::Write>::Error>),
    #[cfg(feature = "client")]
    Reqwest(reqwest::Error),
    #[cfg(feature = "server")]
    Reqwest(()),
}

impl From<ciborium::ser::Error<<&'static [u8] as ciborium_io::Read>::Error>> for RemoteCallError {
    fn from(e: ciborium::ser::Error<<&'static [u8] as ciborium_io::Read>::Error>) -> Self {
        RemoteCallError::CiboriumSerilization(e)
    }
}

impl From<ciborium::de::Error<<Vec<u8> as ciborium_io::Write>::Error>> for RemoteCallError {
    fn from(e: ciborium::de::Error<<Vec<u8> as ciborium_io::Write>::Error>) -> Self {
        RemoteCallError::CiboriumDeserilization(e)
    }
}

#[cfg(feature = "client")]
impl From<reqwest::Error> for RemoteCallError {
    fn from(e: reqwest::Error) -> Self {
        RemoteCallError::Reqwest(e)
    }
}

#[cfg(all(feature = "server", feature = "client"))]
compile_error!("feature \"server\" and feature \"client\" cannot be enabled at the same time");
