#[derive(Debug)]
pub enum Error {
  DiscordError(String),
  TokioReceiveError(String),
  TokioTimeoutError(String)
}

impl std::error::Error for Error { }

impl std::fmt::Display for Error {
  #[track_caller]
  #[inline(always)]
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f,"{}", self)
  }
}

impl From<tokio::sync::watch::error::RecvError> for Error {
  #[track_caller]
  #[inline(always)]
  fn from(error: tokio::sync::watch::error::RecvError) -> Self {
    Self::TokioReceiveError(format!("{:?}", error))
  }
}

impl From<tokio::time::error::Elapsed> for Error {
  #[track_caller]
  #[inline(always)]
  fn from(error: tokio::time::error::Elapsed) -> Self {
    Self::TokioTimeoutError(format!("{:?}", error))
  }
}