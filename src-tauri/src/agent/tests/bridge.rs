use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::agent::bridge::UiBridge;
use crate::agent::types::{AgentConfirmDecision, AgentUiRequest, AgentUiResponse};
use crate::error::AppError;

fn confirm() -> AgentUiRequest {
    AgentUiRequest::Confirm {
        connection_id: "c1".into(),
        server: "Prod/web".into(),
        action: "Run a command".into(),
        detail: "uptime".into(),
    }
}

#[tokio::test]
async fn the_ui_answer_resolves_the_request() {
    let bridge = Arc::new(UiBridge::default());
    let published: Arc<Mutex<Option<String>>> = Arc::default();
    let responder = {
        let bridge = bridge.clone();
        let published = published.clone();
        tokio::spawn(async move {
            loop {
                let id = published.lock().unwrap().clone();
                if let Some(id) = id {
                    let response = AgentUiResponse::Confirm {
                        decision: AgentConfirmDecision::Once,
                    };
                    return bridge.respond(&id, response);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
    };
    let answer = bridge
        .request(
            confirm(),
            Duration::from_secs(5),
            "timeout",
            |event| {
                *published.lock().unwrap() = Some(event.request_id);
                Ok(())
            },
            |_| panic!("an answered request is not retracted"),
        )
        .await
        .unwrap();
    assert!(responder.await.unwrap());
    assert!(matches!(
        answer,
        AgentUiResponse::Confirm {
            decision: AgentConfirmDecision::Once
        }
    ));
}

#[tokio::test]
async fn an_error_answer_becomes_an_error() {
    let bridge = Arc::new(UiBridge::default());
    let request = {
        let bridge = bridge.clone();
        tokio::spawn(async move {
            bridge
                .request(
                    AgentUiRequest::Tabs,
                    Duration::from_secs(5),
                    "timeout",
                    |event| {
                        let bridge = bridge.clone();
                        tokio::spawn(async move {
                            bridge.respond(
                                &event.request_id,
                                AgentUiResponse::Error {
                                    message: "no such tab".into(),
                                },
                            )
                        });
                        Ok(())
                    },
                    |_| {},
                )
                .await
        })
    };
    let error = request.await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), "no such tab");
}

#[tokio::test]
async fn a_timed_out_request_is_forgotten_and_retracted() {
    let bridge = UiBridge::default();
    let mut id = String::new();
    let retracted: Mutex<Option<String>> = Mutex::default();
    let result = bridge
        .request(
            AgentUiRequest::Tabs,
            Duration::from_millis(20),
            "nobody answered",
            |event| {
                id = event.request_id;
                Ok(())
            },
            |request_id| *retracted.lock().unwrap() = Some(request_id.to_string()),
        )
        .await;
    assert_eq!(result.unwrap_err().to_string(), "nobody answered");
    assert_eq!(retracted.lock().unwrap().as_deref(), Some(id.as_str()));
    // A late answer finds nothing to resolve.
    assert!(!bridge.respond(&id, AgentUiResponse::Tabs { tabs: vec![] }));
}

#[tokio::test]
async fn a_cancelled_request_is_retracted() {
    let bridge = Arc::new(UiBridge::default());
    let retracted: Arc<Mutex<Option<String>>> = Arc::default();
    let published: Arc<Mutex<Option<String>>> = Arc::default();
    let waiting = tokio::spawn({
        let (bridge, retracted, published) = (bridge.clone(), retracted.clone(), published.clone());
        async move {
            bridge
                .request(
                    confirm(),
                    Duration::from_secs(30),
                    "timeout",
                    |event| {
                        *published.lock().unwrap() = Some(event.request_id);
                        Ok(())
                    },
                    move |request_id| *retracted.lock().unwrap() = Some(request_id.to_string()),
                )
                .await
        }
    });
    while published.lock().unwrap().is_none() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // The MCP call is cancelled: its future is dropped mid-wait.
    waiting.abort();
    let _ = waiting.await;
    let id = published.lock().unwrap().clone().unwrap();
    assert_eq!(retracted.lock().unwrap().as_deref(), Some(id.as_str()));
    assert!(!bridge.respond(&id, AgentUiResponse::Tabs { tabs: vec![] }));
}

#[tokio::test]
async fn a_failed_publish_does_not_wait_or_leak() {
    let bridge = UiBridge::default();
    let mut id = String::new();
    let result = bridge
        .request(
            AgentUiRequest::Tabs,
            Duration::from_secs(30),
            "timeout",
            |event| {
                id = event.request_id;
                Err(AppError::Other("no window".into()))
            },
            |_| {},
        )
        .await;
    assert_eq!(result.unwrap_err().to_string(), "no window");
    assert!(!bridge.respond(&id, AgentUiResponse::Tabs { tabs: vec![] }));
}
