#[macro_export]
macro_rules! assert_event_sequence {
    ($events:expr, $expected_len:expr) => {{
        let events = &$events;
        let len = events.len();
        assert_eq!(
            len,
            $expected_len,
            "Expected {} events, but got {}: {:?}",
            $expected_len,
            len,
            events
        );
    }};
    ($events:expr, $expected_len:expr, $( $pattern:pat ),*) => {{
        let events = &$events;
        let len = events.len();
        assert_eq!(
            len,
            $expected_len,
            "Expected {} events, but got {}: {:?}",
            $expected_len,
            len,
            events
        );
        let mut index = 0;
        $(
            let event = &events[index];
            assert!(
                matches!(event, $pattern),
                "Event {} did not match pattern {:?}, got: {:?}",
                index,
                stringify!($pattern),
                event
            );
            index += 1;
        )*
    }};
}

#[macro_export]
macro_rules! assert_event_occurs {
    ($events:expr, $event_type:ty) => {{
        let events = &$events;
        let found = events.iter().any(|e| matches!(e, _ as $event_type));
        assert!(
            found,
            "Expected event of type {} but not found in {:?}",
            stringify!($event_type),
            events
        );
    }};
}

#[macro_export]
macro_rules! assert_no_event {
    ($events:expr, $event_type:ty) => {{
        let events = &$events;
        let found = events.iter().any(|e| matches!(e, _ as $event_type));
        assert!(
            !found,
            "Did not expect event of type {} but found in {:?}",
            stringify!($event_type),
            events
        );
    }};
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(tag = "type")]
    enum TestEvent {
        Start { id: String },
        Progress { percent: u32 },
        Complete { result: String },
        Error { message: String },
    }

    #[test]
    fn test_assert_event_sequence_exact_length() {
        let events = vec![
            TestEvent::Start {
                id: "1".to_string(),
            },
            TestEvent::Progress { percent: 50 },
        ];
        assert_event_sequence!(events, 2);
    }

    #[test]
    fn test_assert_event_sequence_length_mismatch() {
        let events = vec![TestEvent::Start {
            id: "1".to_string(),
        }];
        let result = std::panic::catch_unwind(|| {
            assert_event_sequence!(events, 3);
        });
        assert!(result.is_err());
    }
}
