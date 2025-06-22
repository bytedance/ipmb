#!/usr/bin/env nu

export def setup [] {
    npm i -g @napi-rs/cli@2.18.4
}

export def pack [archive: string, ...files: string] {
    if (sys host).name == "Windows" {
        let command = $"Compress-Archive -Path ($files | str join ', ') -DestinationPath ($archive)"

        if (which pwsh | is-empty) {
            powershell -Command $command
        } else {
            pwsh -Command $command
        }
    } else {
        ^zip -r $archive ...$files
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
        (napi build 
            -p ipmb-js 
            --cargo-cwd ipmb-js 
            -c ipmb-js/package.json
            --dts ipmb-js/index.d.ts 
            --target $target 
            --release
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
        if $ignore_rust_version {
            cargo build -p ipmb-ffi --target $target --release --ignore-rust-version
        } else {
            cargo build -p ipmb-ffi --target $target --release
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
    cmake .
    cmake --build . --target cc_client
	./target/debug/cc_client
}

export def "demo cc plus" [] {
    cargo build -p ipmb-ffi
    cmake .
    cmake --build . --target cc_client_plus
	./target/debug/cc_client_plus
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
