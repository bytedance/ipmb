#!/usr/bin/env nu

export def setup [] {
    npm i -g @napi-rs/cli@2.18.4
}

export def pack [dest: string ...files: string] {
    if $nu.os-info.name == "windows" {
        run-external C:\Windows\System32\tar.exe "-a" "-cf" $dest ...$files
    } else {
        ^zip -r $dest ...$files
    }
}

export def test [...targets: string] {
    for target in $targets {
        rustup target add $target;
        cargo run --target $target --example reliability
    }
}

export def "build js" [...targets: string] {
    setup
    let version = (open ipmb-js/Cargo.toml).package.version
    let pwd = ($env.PWD)

    for target in $targets {
        let family = if ($target | str contains "darwin") {
            'darwin'
        } else if ($target | str contains "windows") {
            'windows'
        } else if ($target | str contains "linux") {
            'linux'
        } else {
            continue
        }

        rustup target add $target

        # Build
        mut args = []
        if ($target | str contains "linux") {
            $args = $args | append [--zig --zig-abi-suffix=2.27]
            cargo install cargo-zigbuild
        }

        (napi build 
            -p ipmb-js 
            --cargo-cwd ipmb-js 
            -c ipmb-js/package.json
            --dts ipmb-js/index.d.ts 
            --target $target 
            --release
            ...$args
        )
        mkdir $"ipmb-js/($target)/release/"
	    mv ipmb_js.node $"ipmb-js/($target)/release/"

        # Pack symbols
        cd $"target/($target)/release/"
        for name in [ipmb_js] {
            mut sym: any = null
            match $family {
                'darwin' => {
                    $sym = $"lib($name).dylib.dSYM-v($version)-($target).zip"
                    pack $sym $"lib($name).dylib.dSYM" 
                }
                'windows' => {
                    $sym = $"($name)-v($version)-($target).pdb"
                    cp $"($name).pdb" $sym
                }
                'linux' => {
                    $sym = $'lib($name)-v($version)-($target).so.dwp'
                    cp $'lib($name).so.dwp' $sym
                }
            }
            mv $sym ../../
        }
        cd $pwd
    }
}

export def "dist js" [] {
    cd ipmb-js

    $"//registry.npmjs.org/:_authToken=($env.NPM_TOKEN)" | save .npmrc -f
    npm publish 

    cd ..
}

export def "build ffi" [--ignore-rust-version ...targets: string] {
    let version = (open ipmb-ffi/Cargo.toml).package.version
    let pwd = ($env.PWD)

    for target in $targets {
        let family = if ($target | str contains "darwin") {
            'darwin'
        } else if ($target | str contains "windows") {
            'windows'
        } else if ($target | str contains "linux") {
            'linux'
        } else {
            continue
        }

        rustup target add $target

        # Build
        mut args = [-p ipmb-ffi --release]
        if $ignore_rust_version {
            $args = $args | append "--ignore-rust-version"
        }

        if ($target | str contains "linux") {
            cargo install cargo-zigbuild
            cargo zigbuild --target $"($target).2.27" ...$args
        } else {
            cargo build --target $target ...$args
        }

        # Pack symbols
        cd $"target/($target)/release/";
        for name in [ipmb_ffi] {
            mut sym: any = null
            match $family {
                'darwin' => {
                    $sym = $"lib($name).dylib.dSYM-v($version)-($target).zip"
                    pack $sym $"lib($name).dylib.dSYM" 
                }
                'windows' => {
                    $sym = $"($name)-v($version)-($target).pdb"
                    cp $"($name).pdb" $sym
                }
                'linux' => {
                    $sym = $'lib($name)-v($version)-($target).so.dwp'
                    cp $'lib($name).so.dwp' $sym
                }
            }
            mv $sym ../../
        }
        cd $pwd

        # Pack artifacts
        mut dy: any = null
        match $family {
            'darwin' => {
                $dy = [libipmb_ffi.dylib]
            }
            'windows' => {
                $dy = [ipmb_ffi.dll ipmb_ffi.dll.lib]
            }
            'linux' => {
                $dy = [libipmb_ffi.so]
            }
        }

        for name in $dy {
            cp $"target/($target)/release/($name)" ipmb-ffi/
        }
        cd ipmb-ffi
        pack $"ipmb-ffi-v($version)-($target).zip" include/ ipmb.cc ...$dy 
        for name in $dy {
            rm $name 
        }
        cd $pwd
    }
}

export def "demo cc" [] {
    cargo build -p ipmb-ffi
    cmake -B build -S .
    cmake --build build --target cc_client
	./build/cc_client
}

export def "demo cc plus" [] {
    cargo build -p ipmb-ffi
    cmake -B build -S .
    cmake --build build --target cc_client_plus
	./build/cc_client
}

export def "demo js" [] {
    (napi build
		-p ipmb-js
		--cargo-cwd ipmb-js
		-c ipmb-js/package.json
		--dts ipmb-js/index.d.ts
    )
	mv ipmb_js.node target/debug/

	node ipmb-js/examples/node_client.js
}

export def fmt [] {
    cargo +nightly fmt
}
