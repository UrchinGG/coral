use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::entry::{Caller, Cause, Outcome};

tokio::task_local! {
    static CONTEXT: Arc<RequestContext>;
}

#[derive(Debug, Default)]
pub struct RequestContext {
    recorded: Mutex<Recorded>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub caller: Option<Caller>,
    pub cause: Option<Cause>,
    pub outcome: Option<Outcome>,
    pub server_error: Option<String>,
}

impl RequestContext {
    pub fn take(&self) -> Recorded {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Recorded> {
        self.recorded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub async fn scope<F: Future>(context: Arc<RequestContext>, request: F) -> F::Output {
    CONTEXT.scope(context, request).await
}

pub fn record_caller(caller: Caller) {
    update(|recorded| recorded.caller = Some(caller));
}

pub fn record_cause(cause: Cause) {
    update(|recorded| recorded.cause = Some(cause));
}

pub fn record_outcome(outcome: Outcome) {
    update(|recorded| recorded.outcome = Some(outcome));
}

pub fn record_server_error(detail: impl Into<String>) {
    let detail = detail.into();
    update(|recorded| recorded.server_error = Some(detail));
}

fn update(apply: impl FnOnce(&mut Recorded)) {
    if let Ok(context) = CONTEXT.try_with(Arc::clone) {
        apply(&mut context.lock());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::KeyKind;

    #[tokio::test]
    async fn records_inside_the_request_scope() {
        let context = Arc::new(RequestContext::default());
        scope(context.clone(), async {
            record_caller(Caller::member(KeyKind::Personal, 4));
            record_cause(Cause::CallerLimit);
            record_server_error("boom");
        })
        .await;
        let recorded = context.take();
        assert_eq!(recorded.caller, Some(Caller::member(KeyKind::Personal, 4)));
        assert_eq!(recorded.cause, Some(Cause::CallerLimit));
        assert_eq!(recorded.server_error.as_deref(), Some("boom"));
        assert_eq!(context.take(), Recorded::default());
    }

    #[tokio::test]
    async fn later_records_replace_earlier_ones() {
        let context = Arc::new(RequestContext::default());
        scope(context.clone(), async {
            record_caller(Caller::session(None, 9));
            record_caller(Caller::session(Some(2), 9));
        })
        .await;
        assert_eq!(context.take().caller, Some(Caller::session(Some(2), 9)));
    }

    #[test]
    fn records_outside_a_request_are_ignored() {
        record_cause(Cause::Internal);
    }
}
