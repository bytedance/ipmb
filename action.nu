#!/usr/bin/env nu

def setup [] {
    npm i -g @napi-rs/cli@2.14.8
}

def pack [archive: string, ...files: string] {
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

def test [...targets: string] {
    for target in $targets {
        rustup target add $target;
        cargo run --target $target --example reliability
    }
}

def "publish js" [] {

}

def "demo cc" [] {
    cmake .
    make cc_client
	./target/debug/cc_client
}

def "demo cc plus" [] {
    cmake .
    make cc_client_plus
	./target/debug/cc_client_plus
}

def "demo js" [] {
    (napi build
		-p ipmb-js
		--cargo-cwd ipmb-js
		-c ipmb-js/package.json
		--dts ipmb-js/index.d.ts
    )
	mv ipmb_js.node target/debug/

	node ipmb-js/examples/node_client.js
}
