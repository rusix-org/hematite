#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::capability::{with_model, Config, Owner, Rights, Handle, Model};
const CONFIG: Config = Config { byte_budget: 16, max_in_flight: 2, generation_limit: 2 };

pub fn client(model: &Model<'_>) { let _ = <Model<'_> as Clone>::clone(model); }
