use arm::logic_instance::LogicInstance;
use risc0_zkvm::guest::env;

fn main() {
    let instance: LogicInstance = env::read();
    env::commit(&instance);
}
