use std::collections::BTreeSet;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Manager, Peripheral as PlatformPeripheral};
use futures::{Sink, Stream, StreamExt};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::warn;
use uuid::Uuid;

use crate::domain::ids::BleAddress;
use crate::error::{ConnectError, PairingHint};
use crate::transport::{BoxedTransport, TransportError};

pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x6ba1_b218_15a8_461f_9fa8_5dca_e273_eafd);
pub const TORADIO_UUID: Uuid = Uuid::from_u128(0xf75c_76d2_129e_4dad_a1dd_7866_1244_01e7);
pub const FROMRADIO_UUID: Uuid = Uuid::from_u128(0x2c55_e69e_4993_11ed_b878_0242_ac12_0002);
pub const FROMNUM_UUID: Uuid = Uuid::from_u128(0xed9d_a18c_a800_4f66_a670_aa75_47e3_4453);

const SCAN_DURATION: Duration = Duration::from_secs(3);

pub struct Discovered {
    pub name: String,
    pub address: BleAddress,
    pub rssi_dbm: Option<i16>,
    pub is_paired: bool,
}

pub async fn scan(duration: Duration) -> Result<Vec<Discovered>, ConnectError> {
    let manager = Manager::new().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let adapter = first_adapter(&manager).await?;
    adapter
        .start_scan(ScanFilter { services: vec![SERVICE_UUID] })
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    sleep(duration).await;

    let peripherals =
        adapter.peripherals().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let mut out = Vec::with_capacity(peripherals.len());
    for p in peripherals {
        let Ok(Some(props)) = p.properties().await else { continue };
        let is_paired = p.is_connected().await.unwrap_or(false);
        out.push(Discovered {
            name: props.local_name.unwrap_or_else(|| "Meshtastic".into()),
            address: BleAddress::new(p.address().to_string()),
            rssi_dbm: props.rssi,
            is_paired,
        });
    }
    let _ = adapter.stop_scan().await;
    Ok(out)
}

pub async fn connect(address: &BleAddress) -> Result<BoxedTransport, ConnectError> {
    let manager = Manager::new().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let adapter = first_adapter(&manager).await?;
    adapter
        .start_scan(ScanFilter { services: vec![SERVICE_UUID] })
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    sleep(SCAN_DURATION).await;

    let peripherals =
        adapter.peripherals().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let _ = adapter.stop_scan().await;

    let peripheral = peripherals
        .into_iter()
        .find(|p| p.address().to_string().eq_ignore_ascii_case(address.as_str()))
        .ok_or_else(|| ConnectError::BleDeviceNotFound(address.as_str().into()))?;

    peripheral.connect().await.map_err(|e| map_connect_error(&e.to_string()))?;
    peripheral
        .discover_services()
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;

    let chars = peripheral.characteristics();
    let to_radio = find_char(&chars, TORADIO_UUID)?;
    let from_radio = find_char(&chars, FROMRADIO_UUID)?;
    let from_num = find_char(&chars, FROMNUM_UUID)?;

    if !from_num.properties.contains(CharPropFlags::NOTIFY) {
        return Err(ConnectError::BleGatt("fromNum has no NOTIFY property".into()));
    }
    peripheral.subscribe(&from_num).await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;

    let transport = BleTransport::spawn(peripheral, to_radio, from_radio).await?;
    Ok(Box::pin(transport))
}

async fn first_adapter(manager: &Manager) -> Result<btleplug::platform::Adapter, ConnectError> {
    let adapters = manager.adapters().await.map_err(|_| ConnectError::BleAdapterUnavailable)?;
    adapters.into_iter().next().ok_or(ConnectError::BleAdapterUnavailable)
}

fn find_char(chars: &BTreeSet<Characteristic>, uuid: Uuid) -> Result<Characteristic, ConnectError> {
    chars
        .iter()
        .find(|c| c.uuid == uuid)
        .cloned()
        .ok_or_else(|| ConnectError::BleGatt(format!("missing characteristic {uuid}")))
}

fn map_connect_error(msg: &str) -> ConnectError {
    if msg.to_ascii_lowercase().contains("auth") {
        ConnectError::BlePairingRequired(current_hint())
    } else {
        ConnectError::BleGatt(msg.into())
    }
}

const fn current_hint() -> PairingHint {
    #[cfg(target_os = "macos")]
    {
        PairingHint::Macos
    }
    #[cfg(target_os = "windows")]
    {
        PairingHint::Windows
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        PairingHint::LinuxBluetoothctl
    }
}

struct BleTransport {
    out: mpsc::UnboundedSender<Vec<u8>>,
    incoming: mpsc::UnboundedReceiver<Result<Vec<u8>, TransportError>>,
}

impl BleTransport {
    async fn spawn(
        peripheral: PlatformPeripheral,
        to_radio: Characteristic,
        from_radio: Characteristic,
    ) -> Result<Self, ConnectError> {
        let notifications = peripheral
            .notifications()
            .await
            .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
        let (out_tx, out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (in_tx, in_rx) = mpsc::unbounded_channel();

        let reader_peripheral = peripheral.clone();
        let reader_from_radio = from_radio.clone();
        tokio::spawn(read_loop(notifications, reader_peripheral, reader_from_radio, in_tx));

        tokio::spawn(write_loop(out_rx, peripheral, to_radio));

        Ok(Self { out: out_tx, incoming: in_rx })
    }
}

async fn read_loop(
    mut notifications: Pin<Box<dyn Stream<Item = btleplug::api::ValueNotification> + Send>>,
    peripheral: PlatformPeripheral,
    from_radio: Characteristic,
    in_tx: mpsc::UnboundedSender<Result<Vec<u8>, TransportError>>,
) {
    if !drain_from_radio(&peripheral, &from_radio, &in_tx).await {
        return;
    }
    while notifications.next().await.is_some() {
        if !drain_from_radio(&peripheral, &from_radio, &in_tx).await {
            return;
        }
    }
}

async fn drain_from_radio(
    peripheral: &PlatformPeripheral,
    from_radio: &Characteristic,
    in_tx: &mpsc::UnboundedSender<Result<Vec<u8>, TransportError>>,
) -> bool {
    loop {
        match peripheral.read(from_radio).await {
            Ok(bytes) if bytes.is_empty() => return true,
            Ok(bytes) => {
                if in_tx.send(Ok(bytes)).is_err() {
                    return false;
                }
            }
            Err(e) => {
                let _ = in_tx.send(Err(TransportError::Ble(e.to_string())));
                return false;
            }
        }
    }
}

async fn write_loop(
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    peripheral: PlatformPeripheral,
    to_radio: Characteristic,
) {
    while let Some(frame) = out_rx.recv().await {
        if let Err(e) = peripheral.write(&to_radio, &frame, WriteType::WithoutResponse).await {
            warn!(%e, "ble write failed");
            break;
        }
    }
}

impl Stream for BleTransport {
    type Item = Result<Vec<u8>, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.incoming.poll_recv(cx)
    }
}

impl Sink<Vec<u8>> for BleTransport {
    type Error = TransportError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        self.out.send(item).map_err(|_| TransportError::Closed)
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
