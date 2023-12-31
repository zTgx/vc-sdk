use crate::{
	primitives::{
		cerror::CError,
		identity::Identity,
		keypair::KeyPair,
		network::Web3Network,
		signature::validation_data::{
			TwitterValidationData, ValidationData, ValidationString, Web2ValidationData,
			Web3CommonValidationData, Web3ValidationData,
		},
		CResult,
	},
	service::{impls::stf_inner::LinkIdentityInner, json::RpcReturnValue, wsclient::DiRequest},
	utils::{self, hex::FromHexPrefixed},
	Creek, ValidationDataBuilder, WorkerGetters, WorkerSTF,
};
use utils::identity::{get_expected_raw_message, verify_web3_identity};

impl WorkerSTF for Creek {
	fn link_identity(
		&self,
		link_identity: Identity,
		networks: Vec<Web3Network>,
		vdata: ValidationData,
	) -> CResult<()> {
		let shard = self.author_get_shard()?;
		let tee_shielding_key = self.author_get_shielding_key()?;

		let trusted_call_signed =
			self.link_identity_inner(link_identity, networks, &shard, vdata)?;

		let jsonresp =
			self.client().di_request(shard, tee_shielding_key, trusted_call_signed).unwrap();
		let rpc_return_value = RpcReturnValue::from_hex(&jsonresp.result).unwrap();
		println!("[LINK IDENTITY]: {:#?}", rpc_return_value);

		Ok(())
	}
}

impl ValidationDataBuilder for Creek {
	fn twitter_vdata(&self, twitterid: &str) -> CResult<ValidationData> {
		let message = ValidationString::try_from(twitterid.to_string().as_bytes().to_vec())
			.map_err(|_e| CError::Other("Parse sidechain nonce error".to_string()))
			.unwrap();

		Ok(ValidationData::Web2(Web2ValidationData::Twitter(TwitterValidationData {
			tweet_id: message,
		})))
	}

	fn web3_vdata(&self, keypair: &KeyPair) -> CResult<ValidationData> {
		let sidechain_nonce = self.get_sidechain_nonce()?;

		// 1. Get raw message
		let primary = Identity::from(self.signer.account_id());
		let identity = Identity::from(keypair.account_id());
		if identity.is_web2() {
			return Err(CError::Other("Web3 Identity supported ONLY!".to_string()))
		}

		let message_raw = get_expected_raw_message(&primary, &identity, sidechain_nonce);

		// 2. Sign raw message
		let signature = keypair.sign(&message_raw);

		// 3. Build ValidationData
		let web3_common_validation_data = Web3CommonValidationData {
			message: ValidationString::try_from(message_raw.clone()).unwrap(),
			signature,
		};

		match identity {
			Identity::Substrate(_) =>
				Some(Web3ValidationData::Substrate(web3_common_validation_data)),
			Identity::Evm(_) => Some(Web3ValidationData::Evm(web3_common_validation_data)),
			Identity::Bitcoin(_) => Some(Web3ValidationData::Evm(web3_common_validation_data)),
			_ => None,
		}
		.map(|vdata| {
			// 4. Verify
			verify_web3_identity(&identity, &message_raw, &vdata)
				.expect("VerifyWeb3SignatureFailed");

			vdata
		})
		.map(ValidationData::Web3)
		.ok_or(CError::APIError)
	}
}
