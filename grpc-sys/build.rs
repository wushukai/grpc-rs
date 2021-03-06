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

extern crate gcc;
#[cfg(not(feature = "link-sys"))]
extern crate cmake;
extern crate pkg_config;

#[cfg(feature = "link-sys")]
mod imp {
    use gcc::Config as GccConfig;
    use pkg_config::Config as PkgConfig;
    
    const GRPC_VERSION: &'static str = "1.4.0";

    pub fn build_or_link_grpc(cc: &mut GccConfig) {
        if let Ok(lib) = PkgConfig::new().atleast_version(GRPC_VERSION).probe("grpc") {
            for inc_path in lib.include_paths {
                cc.include(inc_path);
            }
        } else {
            panic!("can't find a dynamic grpc library");
        }
    }
}

#[cfg(not(feature = "link-sys"))]
mod imp {
    use std::path::Path;

    use cmake::Config as CMakeConfig;
    use gcc::Config as GccConfig;

    fn prepare_grpc() {
        let modules = vec![
            "grpc",
            "grpc/third_party/zlib",
            "grpc/third_party/boringssl",
            "grpc/third_party/cares/cares",
        ];

        for module in modules {
            if !Path::new(&format!("{}/.git", module)).exists() {
                panic!("Can't find module {}. You need to run `git submodule \
                        update --init --recursive` first to build the project.", module);
            }
        }
    }

    pub fn build_or_link_grpc(cc: &mut GccConfig) {
        prepare_grpc();

        let dst = CMakeConfig::new("grpc")
            .build_target("grpc")
            .build();

        let build_dir = format!("{}/build", dst.display());
        println!("cargo:rustc-link-search=native={}", build_dir);
        println!("cargo:rustc-link-search=native={}/third_party/cares",
                 build_dir);
        println!("cargo:rustc-link-search=native={}/third_party/zlib",
                 build_dir);
        println!("cargo:rustc-link-search=native={}/third_party/boringssl/ssl",
                 build_dir);
        println!("cargo:rustc-link-search=native={}/third_party/boringssl/crypto",
                 build_dir);

        println!("cargo:rustc-link-lib=static=z");
        println!("cargo:rustc-link-lib=static=cares");
        println!("cargo:rustc-link-lib=static=gpr");
        println!("cargo:rustc-link-lib=static=grpc");
        println!("cargo:rustc-link-lib=static=ssl");
        println!("cargo:rustc-link-lib=static=crypto");

        cc.include("grpc/include");
    }
}

fn main() {
    let mut cc = gcc::Config::new();

    imp::build_or_link_grpc(&mut cc);

    cc.file("grpc_wrap.c")
        .flag("-fPIC")
        .flag("-O2")
        .compile("libgrpc_wrap.a");
}
