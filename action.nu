#!/usr/bin/env nu

export def setup [] {
    npm i -g @napi-rs/cli@2.14.8
}

export def pack [archive: string, ...files: string] {
    if (sys | get host | get name) == "Windows" {
        let command = $"Compress-Archive -Path ($files | flatten | str join ', ') -DestinationPath ($archive)"

        if (which pwsh | is-empty) {
            powershell -Command $command
        } else {
            pwsh -Command $command
        }
    } else {
        ^zip -r $archive ($files | flatten)
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
    let version = (open Cargo.toml | get workspace | get package | get version)
    let pwd = ($env.PWD)

    for target in $targets {
        rustup target add $target;

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
        cd $"target/($target)/release/";
        for name in [ipmb-js] {
            let sym = (if ($target | str contains "darwin") {
                let sym = $"lib($name).dylib.dSYM"
                pack $"($sym)-v($version)-($target).zip" $"($sym)" 
                $"($sym)-v($version)-($target).zip"
            } else {
                cp $"($name | str replace - _).pdb" $"($name | str replace - _)-v($version)-($target).pdb"
                $"($name | str replace - _)-v($version)-($target).pdb"
            })
            mv $sym ../../
        }
        cd $pwd;
    }
}

export def "dist js" [] {
    cd ipmb-js

    $"//registry.npmjs.org/:_authToken=($env.NPM_TOKEN)" | save .npmrc -f
    npm publish --dry-run

    cd ..
}

export def "build ffi" [...targets: string] {
    let version = (open Cargo.toml | get workspace | get package | get version)
    let pwd = ($env.PWD)

    for target in $targets {
        rustup target add $target;

        # Build
        cargo build -p ipmb-ffi --target $target --release

        # Pack symbols
        cd $"target/($target)/release/";
        for name in [ipmb-ffi] {
            let sym = (if ($target | str contains "darwin") {
                let sym = $"lib($name).dylib.dSYM"
                pack $"($sym)-v($version)-($target).zip" $"($sym)" 
                $"($sym)-v($version)-($target).zip"
            } else {
                cp $"($name | str replace - _).pdb" $"($name | str replace - _)-v($version)-($target).pdb"
                $"($name | str replace - _)-v($version)-($target).pdb"
            })
            mv $sym ../../
        }
        cd $pwd;

        # Pack artifacts
        let dy = (if ($target | str contains "darwin") {
            [libipmb_ffi.dylib]
        } else {
            [ipmb_ffi.dll ipmb_ffi.dll.lib]
        })
        for name in $dy {
            cp $"target/($target)/release/($name)" ipmb-ffi/
        }
        cd ipmb-ffi
        pack $"ipmb-ffi-v($version)-($target).zip" include/ ipmb.cc $dy 
        for name in $dy {
            rm $name 
        }
        cd $pwd
    }
}

export def "demo cc" [] {
    cmake .
    make cc_client
	./target/debug/cc_client
}

export def "demo cc plus" [] {
    cmake .
    make cc_client_plus
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
