#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayFrame {
    Forward {
        request_id: String,
        target: String,
        payload: Vec<u8>,
    },
    Result {
        request_id: String,
        ok: bool,
        payload: Vec<u8>,
    },
}
