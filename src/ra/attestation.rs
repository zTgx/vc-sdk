/*
    Copyright 2021 Integritee AG and Supercomputing Systems AG

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.

*/

use arrayvec::ArrayVec;
use chrono::DateTime;
use itertools::Itertools;
use serde_json::Value;
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ra::sgx_types::sgx_platform_info_t;
use crate::{
    primitives::{
        enclave::{Enclave, SgxBuildMode},
        AccountId,
    },
    ra::sgx_types::{sgx_quote_t, sgx_status_t, SgxResult, SGX_PLATFORM_INFO_SIZE},
};

pub fn ra_attestation(enclave_registry: &Enclave<AccountId, String>) -> SgxResult<()> {
    println!("enclave registry: {:?}", enclave_registry);

    // 0. check sgx mode : Production
    if enclave_registry.sgx_mode != SgxBuildMode::Production {
        println!("sgx mod MUST BE Production");

        return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
    }

    // 1. Verify quote status (mandatory field)
    let raw_quote = base64::decode(&enclave_registry.sgx_metadata.quote).unwrap();
    let attn_report: Value = match serde_json::from_slice(&raw_quote) {
        Ok(report) => report,
        Err(_) => {
            println!("RA report parsing error");

            return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
        }
    };
    println!("attn_report: {:?}", attn_report);

    // 1. Check timestamp is within 24H (90day is recommended by Intel)
    if let Value::String(time) = &attn_report["timestamp"] {
        let time_fixed = time.clone() + "+0000";
        let ts = DateTime::parse_from_str(&time_fixed, "%Y-%m-%dT%H:%M:%S%.f%z")
            .map_err(|e| {
                println!("{:?}", e);
                sgx_status_t::SGX_ERROR_UNEXPECTED
            })?
            .timestamp();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| {
                println!("{}", e);
                sgx_status_t::SGX_ERROR_UNEXPECTED
            })?
            .as_secs() as i64;
        println!("Time diff = {}", now - ts);
    } else {
        println!("Failed to fetch timestamp from attestation report");

        return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
    }

    // 2. Verify quote status (mandatory field)
    if let Value::String(quote_status) = &attn_report["isvEnclaveQuoteStatus"] {
        println!("isvEnclaveQuoteStatus = {}", quote_status);

        match quote_status.as_ref() {
            "OK" => (),
            "GROUP_OUT_OF_DATE" | "GROUP_REVOKED" | "CONFIGURATION_NEEDED" => {
                // Verify platformInfoBlob for further info if status not OK
                if let Value::String(pib) = &attn_report["platformInfoBlob"] {
                    let mut buf = ArrayVec::<_, SGX_PLATFORM_INFO_SIZE>::new();

                    // the TLV Header (4 bytes/8 hexes) should be skipped
                    let n = (pib.len() - 8) / 2;
                    for i in 0..n {
                        buf.try_push(
							u8::from_str_radix(&pib[(i * 2 + 8)..(i * 2 + 10)], 16)
								.map_err(|e| {
                                    println!("{:?}",e);
                                    sgx_status_t::SGX_ERROR_UNEXPECTED
                                })?,
						)
						.map_err(|e| {
							println!("failed to push element to platform info blob buffer, exceeding buffer size ({})", e);
							sgx_status_t::SGX_ERROR_UNEXPECTED
						})?;
                    }

                    // ArrayVec .into_inner() requires that all elements are occupied by a value
                    // if that's not the case, the following error will occur
                    let platform_info = buf.into_inner().map_err(|e| {
						println!("Failed to extract platform info from InfoBlob, result does not contain enough elements (require: {}, found: {})", e.capacity(), e.len());

						sgx_status_t::SGX_ERROR_UNEXPECTED
					})?;

                    let _platform_info = sgx_platform_info_t { platform_info };
                // attestation_ocall.get_update_info(sgx_platform_info_t { platform_info }, 1)?;
                } else {
                    println!("Failed to fetch platformInfoBlob from attestation report");
                    return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
                }
            }
            status => {
                println!("Unexpected status in attestation report: {}", status);
                return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
            }
        }
    } else {
        println!("Failed to fetch isvEnclaveQuoteStatus from attestation report");
        return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
    }

    // 3. Verify quote body
    if let Value::String(quote_raw) = &attn_report["isvEnclaveQuoteBody"] {
        let quote = base64::decode(quote_raw).map_err(|e| {
            println!("{:?}", format!("{:?}", e));
            sgx_status_t::SGX_ERROR_UNEXPECTED
        })?;
        println!("Quote = {:?}", quote);

        // TODO: lack security check here
        let sgx_quote: sgx_quote_t = unsafe { ptr::read(quote.as_ptr() as *const _) };

        // let ti = attestation_ocall.get_mrenclave_of_self()?;
        // if sgx_quote.report_body.mr_enclave.m != ti.m {
        // 	error!(
        // 		"mr_enclave is not equal to self {:?} != {:?}",
        // 		sgx_quote.report_body.mr_enclave.m, ti.m
        // 	);
        // 	return Err(sgx_status_t::SGX_ERROR_UNEXPECTED)
        // }

        // ATTENTION
        // DO SECURITY CHECK ON DEMAND
        // DO SECURITY CHECK ON DEMAND
        // DO SECURITY CHECK ON DEMAND

        // Curly braces to copy `unaligned_references` of packed fields into properly aligned temporary:
        // https://github.com/rust-lang/rust/issues/82523
        println!("sgx quote version = {}", { sgx_quote.version });
        println!("sgx quote signature type = {}", { sgx_quote.sign_type });
        println!(
            "sgx quote report_data = {:02x}",
            sgx_quote.report_body.report_data.d.iter().format("")
        );
        println!(
            "sgx quote mr_enclave = {:02x}",
            sgx_quote.report_body.mr_enclave.m.iter().format("")
        );
        println!(
            "sgx quote mr_signer = {:02x}",
            sgx_quote.report_body.mr_signer.m.iter().format("")
        );

        // TODO: pubkey???
        // println!("Anticipated public key = {:02x}", pub_k.iter().format(""));
        // if sgx_quote.report_body.report_data.d.to_vec() == pub_k.to_vec() {
        // 	println!("Mutual RA done!");
        // }
    } else {
        println!("Failed to fetch isvEnclaveQuoteBody from attestation report");
        return Err(sgx_status_t::SGX_ERROR_UNEXPECTED);
    }

    Ok(())
}
