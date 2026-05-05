use std::f64;

use jsone::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Payload<N> {
    id: BigInt<N>,
}

fn main() {
    {
        let json = serde_json::to_string(&Payload { id: BigInt(42) }).unwrap();
        assert_eq!(json, r#"{"id":{"$$jsone$remap$$":"42"}}"#);
        println!("{json}");

        let payload: Payload<i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.id, BigInt(42));
        println!("{payload:?}");
    }

    {
        let json = serde_json::to_string(&Payload {
            id: BigInt(f64::NAN),
        })
        .unwrap();
        assert_eq!(json, r#"{"id":{"$$jsone$remap$$":1}}"#);
        println!("{json}");

        let payload: Payload<f64> = serde_json::from_str(&json).unwrap();
        assert!(payload.id.0.is_nan());
        println!("{payload:?}");
    }
}
