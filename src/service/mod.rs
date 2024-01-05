use self::{parachainclient::ParachainRpcClient, workerclient::SidechainRpcClient};
use crate::{
	primitives::{keypair::KeyPair, CResult},
	Creek, CreekExplorer,
};

pub mod getter_trait;
pub mod impls;
pub mod json;
pub mod parachainclient;
pub mod workerclient;

impl CreekExplorer for Creek {
	fn explorer(
		parachain_endpoint: &str,
		worker_endpoint: &str,
		signer: KeyPair,
	) -> CResult<Creek> {
		let parachain_client = ParachainRpcClient::new(parachain_endpoint)?;
		let worker_client = SidechainRpcClient::new(worker_endpoint);

		Ok(Self { parachain_client, worker_client, signer })
	}
}
