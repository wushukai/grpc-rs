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


use std::sync::Arc;

use error::Result;
use grpc::{EnvBuilder, Server as GrpcServer, ServerBuilder, ShutdownFuture};
use grpc_proto::testing::control::{ServerConfig, ServerStatus, ServerType};
use grpc_proto::testing::stats::ServerStats;
use grpc_proto::testing::services_grpc;
use grpc_proto::util as proto_util;

use bench::{self, Benchmark, Generic};
use util::{self, CpuRecorder};

pub struct Server {
    server: GrpcServer,
    recorder: CpuRecorder,
}

impl Server {
    pub fn new(cfg: &ServerConfig) -> Result<Server> {
        let mut builder = EnvBuilder::new();
        let thd_cnt = cfg.get_async_server_threads() as usize;
        if thd_cnt != 0 {
            builder = builder.cq_count(thd_cnt);
        }
        let env = Arc::new(builder.build());
        if cfg.get_core_limit() > 0 {
            println!("server config core limit is set but ignored");
        }
        let service = match cfg.get_server_type() {
            ServerType::ASYNC_SERVER => services_grpc::create_benchmark_service(Benchmark),
            ServerType::ASYNC_GENERIC_SERVER => bench::create_generic_service(Generic),
            _ => unimplemented!(),
        };
        let mut builder = ServerBuilder::new(env).register_service(service);
        builder = if cfg.has_security_params() {
            builder.bind_secure("[::]",
                                cfg.get_port() as u16,
                                proto_util::create_test_server_credentials())
        } else {
            builder.bind("[::]", cfg.get_port() as u16)
        };

        let mut s = builder.build().unwrap();
        s.start();
        Ok(Server {
               server: s,
               recorder: CpuRecorder::new(),
           })
    }

    pub fn get_stats(&mut self, reset: bool) -> ServerStats {
        let sample = self.recorder.cpu_time(reset);

        let mut stats = ServerStats::new();
        stats.set_time_elapsed(sample.real_time);
        stats.set_time_user(sample.user_time);
        stats.set_time_system(sample.sys_time);
        stats.set_total_cpu_time(sample.total_cpu);
        stats.set_idle_cpu_time(sample.idle_cpu);
        stats
    }

    pub fn shutdown(&mut self) -> ShutdownFuture {
        self.server.shutdown()
    }

    pub fn get_status(&self) -> ServerStatus {
        let mut status = ServerStatus::new();
        status.set_port(self.server.bind_addrs()[0].1 as i32);
        status.set_cores(util::cpu_num_cores() as i32);
        status
    }
}
