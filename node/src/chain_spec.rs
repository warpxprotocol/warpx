use polkadot_sdk::*;

use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup, Properties};
use sc_service::{ChainType, GenericChainSpec};
use serde::{Deserialize, Serialize};

use warpx_runtime as runtime;

// The URL for the telemetry server.
// const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = GenericChainSpec<Extensions>;

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	#[serde(alias = "relayChain", alias = "RelayChain")]
	pub relay_chain: String,
	/// The id of the Parachain.
	#[serde(alias = "paraId", alias = "ParaId")]
	pub para_id: u32,
}

pub fn development_config() -> Result<ChainSpec, String> {
	let mut properties = Properties::new();
	properties.insert("tokenSymbol".into(), "HANA".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	Ok(ChainSpec::builder(
		runtime::WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		Extensions { relay_chain: "paseo-local".into(), para_id: 1000 },
	)
	.with_name("Development")
	.with_id("dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.build())
}

pub fn local_testnet_config() -> ChainSpec {
	let mut properties = Properties::new();
	properties.insert("tokenSymbol".into(), "warp".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	#[allow(deprecated)]
	ChainSpec::builder(
		runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-local".into(), para_id: 1000 },
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sc_chain_spec::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_protocol_id("local-testnet")
	.with_properties(properties)
	.build()
}
