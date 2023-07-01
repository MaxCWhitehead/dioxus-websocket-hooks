use std::{cell::RefCell, rc::Rc};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use wasm_sockets::{EventClient, Message, WebSocketError};

#[derive(Clone)]
pub struct DioxusWs {
    event_client: Rc<RefCell<EventClient>>,
    connected: Rc<RefCell<bool>>,
}

pub enum SendError {
    SocketNotConnected(),
    JsError(JsValue),
}

impl From<JsValue> for SendError {
    fn from(value: JsValue) -> Self {
        SendError::JsError(value)
    }
}

impl DioxusWs {
    pub fn new(url: &str) -> Result<DioxusWs, WebSocketError> {
        let mut client = wasm_sockets::EventClient::new(url)?;

        let connected = Rc::new(RefCell::new(false));

        client.set_on_error(Some(Box::new(|error| {
            log::error!("Web socket error: {:?}", error);
        })));
        {
            let connected = connected.clone();
            client.set_on_connection(Some(Box::new(move |_client| {
                let mut borrowed = connected.borrow_mut();
                *borrowed = true;
            })));
        }
        {
            let connected = connected.clone();
            client.set_on_close(Some(Box::new(move |_client| {
                log::info!("Websocket connection closed.");
                let mut borrowed = connected.borrow_mut();
                *borrowed = false;
            })));
        }

        Ok(DioxusWs {
            event_client: Rc::new(RefCell::new(client)),
            connected,
        })
    }

    // Sends a Message
    pub fn send(&self, msg: Message) {
        if *self.connected.borrow() {
            let result = match msg {
                Message::Text(text) => self.event_client.borrow().send_string(&text),
                Message::Binary(binary) => self.event_client.borrow().send_binary(binary),
            };
            if let Err(err) = result {
                log::error!("Error when sending message on websocket: {:?}", err);
            }
        } else {
            log::error!("WebSocket tried to send message when not connected.");
        }
    }

    // Sends a plaintext string
    pub fn send_text(&self, text: String) {
        let msg = Message::Text(text);
        self.send(msg);
    }

    // Sends data that implements Serialize as JSON
    pub fn send_json<T: Serialize>(&self, value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let msg = Message::Text(json);
        self.send(msg)
    }
}

fn log_err(s: &str) {
    web_sys::console::error_1(&JsValue::from_str(s));
}

/// Provide websocket context with a handler for incoming reqwasm Messages
pub fn use_ws_context_provider(cx: &ScopeState, url: &str, handler: impl Fn(Message) + 'static) {
    let handler = Rc::new(handler);

    cx.use_hook(|| {
        let ws = DioxusWs::new(url);
        if let Err(err) = ws {
            log::error!("Error creating WebSocket for {}: {}", url, err);
            return;
        }
        let ws = cx.provide_context(ws.unwrap());
        ws.event_client.borrow_mut().set_on_message(Some(Box::new(
            move |_client: &wasm_sockets::EventClient, message: Message| {
                handler(message);
            },
        )));
    });
}

/// Provide websocket context with a handler for incoming plaintext messages
pub fn use_ws_context_provider_text(
    cx: &ScopeState,
    url: &str,
    handler: impl Fn(String) + 'static,
) {
    let handler = move |msg| {
        if let Message::Text(text) = msg {
            handler(text)
        }
    };

    use_ws_context_provider(cx, url, handler)
}

/// Provide websocket context with a handler for incoming JSON messages.
/// Note that the message type T must implement Deserialize.
pub fn use_ws_context_provider_json<T>(cx: &ScopeState, url: &str, handler: impl Fn(T) + 'static)
where
    T: for<'de> Deserialize<'de>,
{
    let handler = move |msg| match msg {
        Message::Text(text) => {
            let json = serde_json::from_str::<T>(&text);

            match json {
                Ok(json) => handler(json),
                Err(e) => log::error!("Error while deserializing websocket response: {}", e),
            }
        }
        Message::Binary(_) => log::error!("Error: binary websocket message unsupported"),
    };

    use_ws_context_provider(cx, url, handler)
}

/// Consumes WebSocket context. Useful for sending messages over the WebSocket
/// connection.
pub fn use_ws_context(cx: &ScopeState) -> DioxusWs {
    cx.consume_context::<DioxusWs>().unwrap()
}
