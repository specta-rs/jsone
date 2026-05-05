use std::f64;

use jsone::Jsone;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Payload<N> {
    id: N,
}

fn main() {
    {
        // wrap your root type in `Jsone` and we will take care of the rest!
        let json = serde_json::to_string(&Jsone(Payload { id: 42 })).unwrap();
        assert_eq!(json, r#"{"id":42}"#);
        println!("{json}");

        let payload: Jsone<Payload<i32>> = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.0.id, 42);
        println!("{payload:?}");
    }

    {
        // wrap your root type in `Jsone` and we will take care of the rest!
        let json = serde_json::to_string(&Jsone(Payload { id: f64::MAX })).unwrap();
        assert_eq!(
            json,
            format!(r#"{{"id":{{"$$jsone$remap$$":"{}"}}}}"#, f64::MAX)
        );
        println!("\n{json}");

        let payload: Jsone<Payload<f64>> = serde_json::from_str(&json).unwrap();
        assert_eq!(payload.0.id, f64::MAX);
        println!("{payload:?}");
    }

    {
        let json = serde_json::to_string(&Jsone(Payload { id: f64::NAN })).unwrap();
        assert_eq!(json, r#"{"id":{"$$jsone$remap$$":1}}"#);
        println!("\n{json}");

        let payload: Jsone<Payload<f64>> = serde_json::from_str(&json).unwrap();
        assert!(payload.0.id.is_nan());
        println!("{payload:?}");
    }
}
