pub fn parse() -> Args {
    let args = <RawArgs as clap::Parser>::parse();
    match args.try_into() {
        Ok(args) => args,
        Err((kind, msg)) => <RawArgs as clap::CommandFactory>::command()
            .error(kind, msg)
            .exit(),
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum Verbosity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<Verbosity> for tracing::Level {
    fn from(value: Verbosity) -> Self {
        match value {
            Verbosity::Error => tracing::Level::ERROR,
            Verbosity::Warn => tracing::Level::WARN,
            Verbosity::Info => tracing::Level::INFO,
            Verbosity::Debug => tracing::Level::DEBUG,
            Verbosity::Trace => tracing::Level::TRACE,
        }
    }
}

#[derive(Debug)]
pub struct Args {
    pub cors: bool,
    pub verbosity: tracing::Level,
    pub serve_points: std::collections::HashMap<u16, Vec<ServePoint>>,
}

#[derive(Debug, Clone)]
pub enum Target {
    Dir(std::path::PathBuf),
    Net(hyper::Uri),
}

#[derive(Debug, Clone)]
pub struct ServePoint {
    pub path: String,
    pub target: Target,
}

#[derive(Debug, Clone)]
struct RawServePoint {
    port: u16,
    path: String,
    target: Target,
}

impl RawServePoint {
    fn decompose(self) -> (u16, ServePoint) {
        (
            self.port,
            ServePoint {
                path: self.path,
                target: self.target,
            },
        )
    }
}

#[derive(clap::Parser, Debug)]
#[clap(author, version, about, color = clap::ColorChoice::Always, long_about = None)]
struct RawArgs {
    /// Serve points in the `[port]:[path]:[@]target` format
    ///
    /// Port will default to 3030 if omitted.
    /// Path will default to "/" if omitted.
    /// Target will default to path to a directory if not flagged with "@"
    #[arg(required = true, value_parser = parse_serve_point )]
    serve: Vec<RawServePoint>,

    /// Enable CORS headers dismissal
    #[arg(short, long)]
    cors: bool,

    /// Verbosity level
    #[arg(short, long, value_enum, ignore_case = true, default_value = "info")]
    verbosity: Verbosity,
}

impl TryFrom<RawArgs> for Args {
    type Error = (clap::error::ErrorKind, String);

    fn try_from(
        RawArgs {
            serve,
            cors,
            verbosity,
        }: RawArgs,
    ) -> Result<Self, Self::Error> {
        let mut serve_points = std::collections::HashMap::new();
        for serve_point in serve {
            let (port, serve_point) = serve_point.decompose();
            let entry = serve_points.entry(port).or_insert_with(Vec::new);
            if entry
                .iter()
                .any(|existing: &ServePoint| existing.path == serve_point.path)
            {
                return Err((
                    clap::error::ErrorKind::ValueValidation,
                    format!(
                        "Repeated serve point: {path} on port {port}",
                        path = serve_point.path
                    ),
                ));
            }

            entry.push(serve_point);
        }
        Ok(Args {
            serve_points,
            cors,
            verbosity: verbosity.into(),
        })
    }
}

fn parse_serve_point(value: &str) -> Result<RawServePoint, String> {
    let mut parts = value.splitn(3, ':');

    let Some(port) = parts.next() else {
        return Err(String::from("Expected format [port]:[path]:[@]target"));
    };

    let Some(path) = parts.next() else {
        return Err(String::from("Expected format [port]:[path]:[@]target"));
    };

    let Some(target) = parts.next() else {
        return Err(String::from("Expected format [port]:[path]:[@]target"));
    };

    let port = if port.is_empty() {
        3030_u16
    } else {
        match port.parse() {
            Ok(port) => port,
            Err(e) => return Err(format!("Could not parse port number from `{port}`: {e:?}")),
        }
    };

    let path = if path.is_empty() {
        String::from('/')
    } else if path.starts_with('/') {
        String::from(path)
    } else {
        format!("/{path}")
    };

    let target = if let Some(target) = target.strip_prefix('@') {
        match hyper::Uri::try_from(target).map(Target::Net) {
            Ok(target) => target,
            Err(e) => return Err(format!("Could not parse target URI from `{target}`: {e:?}")),
        }
    } else {
        let path = std::path::PathBuf::from(target);
        if path.exists() && path.is_dir() {
            Target::Dir(path)
        } else {
            return Err(format!("Could not find directory at target `{target}`"));
        }
    };

    Ok(RawServePoint { port, path, target })
}
