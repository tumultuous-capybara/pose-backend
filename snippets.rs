fn generate_token() -> Box<[u8; 64]> {
    let mut arr: Box<[u8; 64]> = Box::new([0u8; 64]);
    thread_rng().fill(&mut arr[..]);
    arr
}

let serialized = serde_json::to_string(&m).unwrap();
stream.write_all(serialized.as_bytes());

let deserialized: CliCommand = serde_json::from_str(&serialized).unwrap();

let mut response = String::new();
stream.read_to_string(&mut response);
