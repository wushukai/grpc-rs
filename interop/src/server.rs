// Copyright 2017 PingCAP, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// See the License for the specific language governing permissions and
// limitations under the License.


use grpc::{self, ClientStreamingSink, DuplexSink, RequestStream, RpcContext, RpcStatus,
           RpcStatusCode, ServerStreamingSink, UnarySink, WriteFlags};
use futures::{Async, Future, Poll, Sink, Stream, future, stream};

use grpc_proto::testing::test_grpc::TestService;
use grpc_proto::testing::empty::Empty;
use grpc_proto::testing::messages::{SimpleRequest, SimpleResponse, StreamingInputCallRequest,
                                    StreamingInputCallResponse, StreamingOutputCallRequest,
                                    StreamingOutputCallResponse};
use grpc_proto::util;

enum Error {
    Grpc(grpc::Error),
    Abort,
}

impl From<grpc::Error> for Error {
    fn from(error: grpc::Error) -> Error {
        Error::Grpc(error)
    }
}

#[derive(Clone)]
pub struct InteropTestService;

impl TestService for InteropTestService {
    fn empty_call(&self, ctx: RpcContext, _: Empty, resp: UnarySink<Empty>) {
        let res = Empty::new();
        let f = resp.success(res)
            .map_err(|e| panic!("failed to send response: {:?}", e));
        ctx.spawn(f)
    }

    fn unary_call(&self,
                  ctx: RpcContext,
                  mut req: SimpleRequest,
                  sink: UnarySink<SimpleResponse>) {
        if req.has_response_status() {
            let code = req.get_response_status().get_code();
            let msg = Some(req.take_response_status().take_message());
            let status = RpcStatus::new(code.into(), msg);
            let f = sink.fail(status)
                .map_err(|e| panic!("failed to send response: {:?}", e));
            ctx.spawn(f);
            return;
        }
        let resp_size = req.get_response_size();
        let mut resp = SimpleResponse::new();
        resp.set_payload(util::new_payload(resp_size as usize));
        let f = sink.success(resp)
            .map_err(|e| panic!("failed to send response: {:?}", e));
        ctx.spawn(f)
    }

    fn cacheable_unary_call(&self, _: RpcContext, _: SimpleRequest, _: UnarySink<SimpleResponse>) {
        unimplemented!()
    }

    fn streaming_output_call(&self,
                             ctx: RpcContext,
                             req: StreamingOutputCallRequest,
                             sink: ServerStreamingSink<StreamingOutputCallResponse>) {
        let resps: Vec<Result<_, grpc::Error>> = req.get_response_parameters()
            .into_iter()
            .map(|param| {
                let mut resp = StreamingOutputCallResponse::new();
                resp.set_payload(util::new_payload(param.get_size() as usize));
                Ok((resp, WriteFlags::default()))
            })
            .collect();
        let f = sink.send_all(stream::iter(resps))
            .map(|_| {})
            .map_err(|e| panic!("failed to send response: {:?}", e));
        ctx.spawn(f)
    }

    fn streaming_input_call(&self,
                            ctx: RpcContext,
                            stream: RequestStream<StreamingInputCallRequest>,
                            sink: ClientStreamingSink<StreamingInputCallResponse>) {
        let f = stream
            .fold(0,
                  |s, req| Ok(s + req.get_payload().get_body().len()) as grpc::Result<_>)
            .and_then(|s| {
                let mut resp = StreamingInputCallResponse::new();
                resp.set_aggregated_payload_size(s as i32);
                sink.success(resp)
            })
            .map_err(|e| match e {
                         grpc::Error::RemoteStopped => {}
                         e => println!("failed to send streaming inptu: {:?}", e),
                     });
        ctx.spawn(f)
    }

    fn full_duplex_call(&self,
                        ctx: RpcContext,
                        stream: RequestStream<StreamingOutputCallRequest>,
                        sink: DuplexSink<StreamingOutputCallResponse>) {
        let f = stream
            .map_err(Error::Grpc)
            .fold(sink, |sink, mut req| {
                let mut failure = None;
                let mut send = None;
                if req.has_response_status() {
                    let code = req.get_response_status().get_code();
                    let msg = Some(req.take_response_status().take_message());
                    let status = RpcStatus::new(code.into(), msg);
                    failure = Some(sink.fail(status));
                } else {
                    let mut resp = StreamingOutputCallResponse::new();
                    if let Some(param) = req.get_response_parameters().get(0) {
                        resp.set_payload(util::new_payload(param.get_size() as usize));
                    }
                    send = Some(sink.send((resp, WriteFlags::default())));
                }
                future::poll_fn(move || -> Poll<DuplexSink<StreamingOutputCallResponse>, Error> {
                    if let Some(ref mut send) = send {
                        let sink = try_ready!(send.poll());
                        Ok(Async::Ready(sink))
                    } else {
                        try_ready!(failure.as_mut().unwrap().poll());
                        Err(Error::Abort)
                    }
                })
            })
            .and_then(|mut sink| future::poll_fn(move || sink.close().map_err(Error::from)))
            .map_err(|e| match e {
                         Error::Grpc(grpc::Error::RemoteStopped) |
                         Error::Abort => {}
                         Error::Grpc(e) => println!("failed to handle duplex call: {:?}", e),
                     });
        ctx.spawn(f)
    }

    fn half_duplex_call(&self,
                        _: RpcContext,
                        _: RequestStream<StreamingOutputCallRequest>,
                        _: DuplexSink<StreamingOutputCallResponse>) {
        unimplemented!()
    }

    fn unimplemented_call(&self, ctx: RpcContext, _: Empty, sink: UnarySink<Empty>) {
        let f = sink.fail(RpcStatus::new(RpcStatusCode::Unimplemented, None))
            .map_err(|e| println!("failed to report unimplemented method: {:?}", e));
        ctx.spawn(f)
    }
}
