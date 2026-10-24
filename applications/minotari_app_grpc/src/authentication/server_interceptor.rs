//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use log::*;
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_utilities::SafePassword;
use tonic::{Request, Status, codegen::http::header::AUTHORIZATION, service::Interceptor};

use crate::authentication::{BasicAuthCredentials, salted_password::create_salted_hashed_password};

const LOG_TARGET: &str = "applications::minotari_app_grpc::authentication";
const AUTH_FAILURE_WARN_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct WarnLimiter {
    state: Mutex<(Option<Instant>, u64)>,
}

impl WarnLimiter {
    fn should_warn(&self) -> Option<u64> {
        let now = Instant::now();
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (last_warning, num_suppressed) = &mut *state;
        let may_warn =
            last_warning.is_none_or(|last| now.saturating_duration_since(last) >= AUTH_FAILURE_WARN_INTERVAL);
        if may_warn {
            *last_warning = Some(now);
            let suppressed = *num_suppressed;
            *num_suppressed = 0;
            Some(suppressed)
        } else {
            *num_suppressed = num_suppressed.saturating_add(1);
            None
        }
    }
}

#[derive(Debug)]
pub struct ServerAuthenticationInterceptor {
    auth: GrpcAuthentication, // this contains a hashed PHC password in the case of basic authentication
    warn_limiter: Arc<WarnLimiter>,
}

impl ServerAuthenticationInterceptor {
    pub fn new(auth: GrpcAuthentication) -> Option<Self> {
        // The server password needs to be hashed for later client verification
        let processed_auth = match auth {
            GrpcAuthentication::None => auth,
            GrpcAuthentication::Basic { username, password } => GrpcAuthentication::Basic {
                username,
                password: create_salted_hashed_password(password.reveal()).ok()?.as_str().into(),
            },
        };

        Some(Self {
            auth: processed_auth,
            warn_limiter: Arc::default(),
        })
    }

    #[allow(clippy::result_large_err)]
    fn handle_basic_auth(
        &self,
        req: Request<()>,
        valid_username: &str,
        valid_phc_password: &SafePassword,
    ) -> Result<Request<()>, Status> {
        match req.metadata().get(AUTHORIZATION.as_str()) {
            Some(t) => {
                // Parse the provided header
                let header = t.to_str().map_err(|e| self.unauthenticated(e))?;
                let (header_username, header_password) =
                    BasicAuthCredentials::parse_header(header).map_err(|e| self.unauthenticated(e))?;

                // Validate the header credentials against the valid credentials
                let valid_credentials =
                    BasicAuthCredentials::new(valid_username.to_owned(), valid_phc_password.clone())
                        .map_err(|e| self.unauthenticated(e))?;
                valid_credentials
                    .constant_time_validate(&header_username, &header_password)
                    .map_err(|e| self.unauthenticated(e))?;

                Ok(req)
            },
            _ => Err(self.unauthenticated("Missing authorization header")),
        }
    }

    /// Standard unauthenticated response with rate-limited warning
    fn unauthenticated<E: fmt::Display>(&self, err: E) -> Status {
        if let Some(suppressed) = self.warn_limiter.should_warn() {
            if suppressed == 0 {
                warn!(target: LOG_TARGET, "GRPC authentication failed: {err}");
            } else {
                warn!(
                    target: LOG_TARGET,
                    "GRPC authentication failed: {err} ({suppressed} similar warnings suppressed)"
                );
            }
        }
        Status::unauthenticated("Auth failed")
    }
}

impl Interceptor for ServerAuthenticationInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        match &self.auth {
            GrpcAuthentication::None => Ok(request),
            GrpcAuthentication::Basic {
                username,
                password: phc_password,
            } => self.handle_basic_auth(request, username, phc_password),
        }
    }
}

impl Clone for ServerAuthenticationInterceptor {
    fn clone(&self) -> Self {
        Self {
            auth: self.auth.clone(),
            warn_limiter: self.warn_limiter.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::OnceLock};

    use tonic::metadata::MetadataValue;

    use super::*;
    use crate::authentication::basic_auth::MAX_ECHOED_SCHEME_LEN;

    thread_local! {
        static CAPTURE_THREAD: Cell<bool> = const { Cell::new(false) };
    }

    struct CapturedLogger {
        lines: Mutex<Vec<String>>,
    }

    impl log::Log for CapturedLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn && metadata.target() == LOG_TARGET
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) && CAPTURE_THREAD.with(Cell::get) {
                self.lines
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(record.args().to_string());
            }
        }

        fn flush(&self) {}
    }

    static TEST_LOGGER: OnceLock<CapturedLogger> = OnceLock::new();

    fn captured_logger() -> &'static CapturedLogger {
        let logger = TEST_LOGGER.get_or_init(|| CapturedLogger {
            lines: Mutex::new(Vec::new()),
        });
        let _ = log::set_logger(logger);
        log::set_max_level(log::LevelFilter::Warn);
        logger
    }

    #[test]
    fn truncates_overlong_scheme_and_throttles_auth_failure_warnings() {
        let logger = captured_logger();
        logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
        CAPTURE_THREAD.with(|c| c.set(true));

        let mut interceptor = ServerAuthenticationInterceptor::new(GrpcAuthentication::Basic {
            username: "admin".to_string(),
            password: SafePassword::from("secret"),
        })
        .unwrap();

        let token = "A".repeat(8 * 1024);
        let header_val = MetadataValue::try_from(format!("{token} x")).unwrap();

        for _ in 0..1000 {
            let mut req = Request::new(());
            req.metadata_mut().insert(AUTHORIZATION.as_str(), header_val.clone());
            let err = interceptor.call(req).unwrap_err();
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }

        CAPTURE_THREAD.with(|c| c.set(false));
        let lines = logger.lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(lines.len(), 1);
        let echoed_expected = "A".repeat(MAX_ECHOED_SCHEME_LEN);
        let echoed_too_long = "A".repeat(MAX_ECHOED_SCHEME_LEN + 1);
        assert!(lines[0].contains(&echoed_expected));
        assert!(!lines[0].contains(&echoed_too_long));
    }
}
