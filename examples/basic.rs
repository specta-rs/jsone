use std::f64;

use jsone::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Payload<N> {
    id: N,
}

fn main() {
    {
        // wrap your root type in `BigInt` and we will take care of the rest!
        let json = serde_json::to_string(&BigInt(Payload { id: 42 })).unwrap();
        assert_eq!(json, r#"{"id":{"$$jsone$remap$$":"42"}}"#);
        println!("{json}");

        let payload: BigInt<Payload<i32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.0.id, 42);
        println!("{payload:?}");
    }

    {
        let json = serde_json::to_string(&BigInt(Payload { id: f64::NAN })).unwrap();
        assert_eq!(json, r#"{"id":{"$$jsone$remap$$":1}}"#);
        println!("{json}");

        let payload: BigInt<Payload<f64>> = serde_json::from_str(&json).unwrap();
        assert!(payload.0.id.is_nan());
        println!("{payload:?}");
    }
}
