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

export def "publish js" [] {

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
