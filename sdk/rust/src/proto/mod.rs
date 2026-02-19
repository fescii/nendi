//! Handcrafted gRPC bindings for the Nendi stream proto service.
//!
//! Uses the same `tonic::codegen::*` pattern that tonic-build 0.14.x generates,
//! so types compile correctly against tonic 0.14.x.

#![allow(clippy::wildcard_imports)]
use tonic::codegen::*;

// ─── Message types (match proto/stream/types.proto) ──────────────────────────

/// Operation kind on a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, prost::Enumeration)]
#[repr(i32)]
pub enum OpKind {
  OpUnspecified = 0,
  Insert = 1,
  Update = 2,
  Delete = 3,
  Truncate = 4,
}

/// A single column value.
#[derive(Clone, PartialEq, prost::Message)]
pub struct ColumnValue {
  #[prost(string, tag = "1")]
  pub name: String,
  #[prost(uint32, tag = "2")]
  pub oid: u32,
  #[prost(string, optional, tag = "3")]
  pub value: Option<String>,
}

/// A row of column values.
#[derive(Clone, PartialEq, prost::Message)]
pub struct RowData {
  #[prost(message, repeated, tag = "1")]
  pub columns: Vec<ColumnValue>,
}

/// Fully qualified table identifier.
#[derive(Clone, PartialEq, prost::Message)]
pub struct TableId {
  #[prost(string, tag = "1")]
  pub schema: String,
  #[prost(string, tag = "2")]
  pub table: String,
}

/// A single change event from the WAL.
#[derive(Clone, PartialEq, prost::Message)]
pub struct ChangeEvent {
  #[prost(uint64, tag = "1")]
  pub lsn: u64,
  #[prost(uint32, tag = "2")]
  pub xid: u32,
  #[prost(enumeration = "OpKind", tag = "3")]
  pub op: i32,
  #[prost(message, optional, tag = "4")]
  pub table: Option<TableId>,
  #[prost(uint64, tag = "5")]
  pub fingerprint: u64,
  #[prost(message, optional, tag = "6")]
  pub before: Option<RowData>,
  #[prost(message, optional, tag = "7")]
  pub after: Option<RowData>,
  #[prost(uint64, tag = "8")]
  pub committed: u64,
}

// ─── Service messages (match proto/stream/service.proto) ─────────────────────

/// Request to subscribe to a change event stream.
#[derive(Clone, PartialEq, prost::Message)]
pub struct SubscribeRequest {
  #[prost(string, tag = "1")]
  pub consumer: String,
  #[prost(uint64, tag = "2")]
  pub resume: u64,
  #[prost(string, repeated, tag = "3")]
  pub tables: Vec<String>,
  #[prost(uint32, tag = "4")]
  pub credits: u32,
}

/// Request to commit a consumer offset.
#[derive(Clone, PartialEq, prost::Message)]
pub struct CommitRequest {
  #[prost(string, tag = "1")]
  pub consumer: String,
  #[prost(uint64, tag = "2")]
  pub lsn: u64,
}

/// Response to a commit request.
#[derive(Clone, PartialEq, prost::Message)]
pub struct CommitResponse {
  #[prost(bool, tag = "1")]
  pub ok: bool,
  #[prost(string, tag = "2")]
  pub error: String,
}

/// Request to get consumer status.
#[derive(Clone, PartialEq, prost::Message)]
pub struct StatusRequest {
  #[prost(string, tag = "1")]
  pub consumer: String,
}

/// Consumer status response.
#[derive(Clone, PartialEq, prost::Message)]
pub struct StatusResponse {
  #[prost(string, tag = "1")]
  pub consumer: String,
  #[prost(uint64, tag = "2")]
  pub committed: u64,
  #[prost(uint64, tag = "3")]
  pub head: u64,
  #[prost(uint64, tag = "4")]
  pub lag: u64,
  #[prost(bool, tag = "5")]
  pub connected: bool,
}

// ─── Custom prost Codec (tonic 0.14 removed ProstCodec from public API) ─────

/// A thin `tonic::codec::Codec` implementation over any prost `Message`.
struct NendiCodec<E, D>(std::marker::PhantomData<(E, D)>);

impl<E, D> Default for NendiCodec<E, D> {
  fn default() -> Self {
    Self(std::marker::PhantomData)
  }
}

impl<E, D> tonic::codec::Codec for NendiCodec<E, D>
where
  E: prost::Message + Send + 'static,
  D: prost::Message + Default + Send + 'static,
{
  type Encode = E;
  type Decode = D;
  type Encoder = NendiEncoder<E>;
  type Decoder = NendiDecoder<D>;

  fn encoder(&mut self) -> Self::Encoder {
    NendiEncoder(std::marker::PhantomData)
  }
  fn decoder(&mut self) -> Self::Decoder {
    NendiDecoder(std::marker::PhantomData)
  }
}

struct NendiEncoder<T>(std::marker::PhantomData<T>);
struct NendiDecoder<T>(std::marker::PhantomData<T>);

impl<T: prost::Message> tonic::codec::Encoder for NendiEncoder<T> {
  type Item = T;
  type Error = tonic::Status;

  fn encode(&mut self, item: T, buf: &mut tonic::codec::EncodeBuf<'_>) -> Result<(), Self::Error> {
    use bytes::BufMut;
    let b = item.encode_to_vec();
    buf.reserve(b.len());
    buf.put_slice(&b);
    Ok(())
  }
}

impl<T: prost::Message + Default> tonic::codec::Decoder for NendiDecoder<T> {
  type Item = T;
  type Error = tonic::Status;

  fn decode(&mut self, buf: &mut tonic::codec::DecodeBuf<'_>) -> Result<Option<T>, Self::Error> {
    use bytes::Buf;
    let item =
      T::decode(buf.chunk()).map_err(|e| tonic::Status::internal(format!("decode: {e}")))?;
    let len = item.encoded_len();
    buf.advance(len.min(buf.remaining()));
    Ok(Some(item))
  }
}

// ─── gRPC Client (NendiStream service) ───────────────────────────────────────

/// gRPC client for the `NendiStream` service.
///
/// Mirrors the structure that tonic-build 0.14 would generate.
#[derive(Debug, Clone)]
pub struct NendiStreamClient<T> {
  inner: tonic::client::Grpc<T>,
}

impl NendiStreamClient<tonic::transport::Channel> {
  /// Create a new client using a pre-built tonic `Channel`.
  pub fn new(channel: tonic::transport::Channel) -> Self {
    let inner = tonic::client::Grpc::new(channel);
    Self { inner }
  }
}

impl<T> NendiStreamClient<T>
where
  T: tonic::client::GrpcService<tonic::body::Body>,
  T::Error: Into<StdError>,
  T::ResponseBody: Body<Data = Bytes> + std::marker::Send + 'static,
  <T::ResponseBody as Body>::Error: Into<StdError> + std::marker::Send,
{
  /// Subscribe to a stream of change events.
  pub async fn subscribe(
    &mut self,
    request: impl tonic::IntoRequest<SubscribeRequest>,
  ) -> Result<tonic::Response<tonic::codec::Streaming<ChangeEvent>>, tonic::Status> {
    self.inner.ready().await.map_err(|e| {
      tonic::Status::new(
        tonic::Code::Unknown,
        format!("Service was not ready: {}", e.into()),
      )
    })?;
    let codec = NendiCodec::<_, _>::default();
    let path = http::uri::PathAndQuery::from_static("/nendi.stream.NendiStream/Subscribe");
    let mut req = request.into_request();
    req.extensions_mut().insert(tonic::GrpcMethod::new(
      "nendi.stream.NendiStream",
      "Subscribe",
    ));
    self.inner.server_streaming(req, path, codec).await
  }

  /// Commit a consumer offset.
  pub async fn commit_offset(
    &mut self,
    request: impl tonic::IntoRequest<CommitRequest>,
  ) -> Result<tonic::Response<CommitResponse>, tonic::Status> {
    self.inner.ready().await.map_err(|e| {
      tonic::Status::new(
        tonic::Code::Unknown,
        format!("Service was not ready: {}", e.into()),
      )
    })?;
    let codec = NendiCodec::<_, _>::default();
    let path = http::uri::PathAndQuery::from_static("/nendi.stream.NendiStream/CommitOffset");
    let mut req = request.into_request();
    req.extensions_mut().insert(tonic::GrpcMethod::new(
      "nendi.stream.NendiStream",
      "CommitOffset",
    ));
    self.inner.unary(req, path, codec).await
  }

  /// Get the current status of a consumer.
  pub async fn get_status(
    &mut self,
    request: impl tonic::IntoRequest<StatusRequest>,
  ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
    self.inner.ready().await.map_err(|e| {
      tonic::Status::new(
        tonic::Code::Unknown,
        format!("Service was not ready: {}", e.into()),
      )
    })?;
    let codec = NendiCodec::<_, _>::default();
    let path = http::uri::PathAndQuery::from_static("/nendi.stream.NendiStream/GetStatus");
    let mut req = request.into_request();
    req.extensions_mut().insert(tonic::GrpcMethod::new(
      "nendi.stream.NendiStream",
      "GetStatus",
    ));
    self.inner.unary(req, path, codec).await
  }
}
