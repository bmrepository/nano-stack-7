fn main() {
    prost_build::compile_protos(&["proto/device.proto"], &["proto/"]).unwrap();
}
