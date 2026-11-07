//! Build script of every reverse-codegen model shard: writes the shard's models (see
//! `model_gen`).

fn main() {
    model_gen::build_shard();
}
