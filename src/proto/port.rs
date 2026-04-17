use bytes::Bytes;
use prost::Message;
use thiserror::Error;

use crate::proto::meshtastic;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("utf-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("protobuf: {0}")]
    Proto(#[from] prost::DecodeError),
}

#[derive(Debug, Clone)]
pub enum PortPayload {
    Text(String),
    Position(meshtastic::Position),
    NodeInfo(meshtastic::User),
    Telemetry(meshtastic::Telemetry),
    Routing(meshtastic::Routing),
    Admin(meshtastic::AdminMessage),
    Unknown { port: i32, bytes: Bytes },
}

pub fn parse(port: i32, bytes: &[u8]) -> Result<PortPayload, ParseError> {
    use meshtastic::PortNum;
    let Ok(port_num) = PortNum::try_from(port) else {
        return Ok(PortPayload::Unknown { port, bytes: Bytes::copy_from_slice(bytes) });
    };
    Ok(match port_num {
        PortNum::TextMessageApp => PortPayload::Text(String::from_utf8(bytes.to_vec())?),
        PortNum::PositionApp => PortPayload::Position(meshtastic::Position::decode(bytes)?),
        PortNum::NodeinfoApp => PortPayload::NodeInfo(meshtastic::User::decode(bytes)?),
        PortNum::TelemetryApp => PortPayload::Telemetry(meshtastic::Telemetry::decode(bytes)?),
        PortNum::RoutingApp => PortPayload::Routing(meshtastic::Routing::decode(bytes)?),
        PortNum::AdminApp => PortPayload::Admin(meshtastic::AdminMessage::decode(bytes)?),
        _ => PortPayload::Unknown { port, bytes: Bytes::copy_from_slice(bytes) },
    })
}
