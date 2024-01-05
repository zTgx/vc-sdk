use crate::{
	primitives::cerror::CError,
	service::json::{json_resp, JsonResponse},
	CResult,
};
use log::*;
use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
use serde_json::Value;
use std::{
	fmt::Debug,
	sync::mpsc::{channel, Sender as ThreadOut},
};
use ws::{
	connect, util::TcpStream, CloseCode, Error, ErrorKind, Handler, Handshake, Message,
	Result as WsResult, Sender,
};

#[allow(clippy::result_large_err)]
pub trait SidechainHandleMessage {
	type ThreadMessage;

	fn handle_message(
		&self,
		msg: Message,
		out: Sender,
		result: ThreadOut<Self::ThreadMessage>,
	) -> WsResult<()>;
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct GetSidechainRequestHandler;
impl SidechainHandleMessage for GetSidechainRequestHandler {
	type ThreadMessage = Message;

	fn handle_message(
		&self,
		msg: Message,
		out: Sender,
		result: ThreadOut<Self::ThreadMessage>,
	) -> WsResult<()> {
		info!("Got get_request_msg {}", msg);

		out.close(CloseCode::Normal)
			.unwrap_or_else(|_| warn!("Could not close Websocket normally"));

		let message = serde_json::from_str(msg.as_text()?)
			.map(|v: serde_json::Value| Some(v.to_string()))
			.map_err(|e| Error::new(ErrorKind::Custom(e.into()), "RPC Get invalid message."))?;

		if let Some(message) = message {
			let _ = result.send(Message::from(message));
		}

		Ok(())
	}
}

pub struct SidechainClient<MessageHandler, ThreadMessage> {
	pub out: ws::Sender,
	pub request: String,
	pub result: ThreadOut<ThreadMessage>,
	pub message_handler: MessageHandler,
}

impl<MessageHandler: SidechainHandleMessage> Handler
	for SidechainClient<MessageHandler, MessageHandler::ThreadMessage>
{
	fn on_open(&mut self, _: Handshake) -> WsResult<()> {
		info!("sending request: {}", self.request);
		self.out.send(self.request.clone())?;
		Ok(())
	}

	fn on_close(&mut self, code: CloseCode, reason: &str) {
		info!("Connection closing due to ({:?}) {}", code, reason);
		let _ = self.out.shutdown().map_err(|e| {
			error!("shutdown error: {:?}", e);
		});
	}

	fn on_message(&mut self, msg: ws::Message) -> ws::Result<()> {
		info!("msg received = {}", msg);
		self.message_handler.handle_message(msg, self.out.clone(), self.result.clone())
	}

	fn upgrade_ssl_client(
		&mut self,
		sock: TcpStream,
		_: &url::Url,
	) -> ws::Result<SslStream<TcpStream>> {
		let mut builder = SslConnector::builder(SslMethod::tls()).map_err(|e| {
			ws::Error::new(
				ws::ErrorKind::Internal,
				format!("Failed to upgrade client to SSL: {}", e),
			)
		})?;
		builder.set_verify(SslVerifyMode::empty());

		let connector = builder.build();
		connector
			.configure()
			.map_err(|e| {
				let details = format!("{:?}", e);
				ws::Error::new(ws::ErrorKind::Internal, details)
			})?
			.use_server_name_indication(false)
			.verify_hostname(false)
			.connect("", sock)
			.map_err(From::from)
	}
}

#[derive(Debug, Clone, Default)]
pub struct SidechainRpcClient {
	url: String,
}

impl SidechainRpcClient {
	pub fn new(url: &str) -> SidechainRpcClient {
		SidechainRpcClient { url: url.to_string() }
	}

	fn send<MessageHandler>(
		&self,
		jsonreq: String,
		message_handler: MessageHandler,
	) -> CResult<MessageHandler::ThreadMessage>
	where
		MessageHandler: SidechainHandleMessage + Clone + Send + 'static,
		MessageHandler::ThreadMessage: Send + Sync + Debug,
	{
		let (result_in, result_out) = channel();
		connect(self.url.as_str(), |out| SidechainClient {
			out,
			request: jsonreq.clone(),
			result: result_in.clone(),
			message_handler: message_handler.clone(),
		})
		.map_err(|_| CError::APIError)?;

		let message = result_out.recv().map_err(CError::RecvError)?;

		Ok(message)
	}
}

pub trait SidechainRpcRequest {
	fn request(&self, jsonreq: serde_json::Value) -> CResult<JsonResponse>;
}

impl SidechainRpcRequest for SidechainRpcClient {
	fn request(&self, jsonreq: Value) -> CResult<JsonResponse> {
		let message = self.send(jsonreq.to_string(), GetSidechainRequestHandler::default())?;
		json_resp(message.to_string())
	}
}