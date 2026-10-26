#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::capability::{with_model, Config, Owner, Rights, Handle, Model};
const CONFIG: Config = Config { byte_budget: 16, max_in_flight: 2, generation_limit: 2 };

pub fn client() {
    let _ = with_model(CONFIG, |mut first| {
        let handle = first.create(Owner(1), 8, Rights::READ).unwrap();
        let _ = with_model(CONFIG, |mut second| second.release(Owner(1), handle));
    });
}
