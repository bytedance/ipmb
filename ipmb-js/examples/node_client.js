const { join, SelectorMode, LabelOp } = require('../../target/debug/ipmb_js.node');

let { sender, receiver } = join({
    identifier: "solar.com",
    label: ["cc"],
    token: "",
    controllerAffinity: true,
}, null);

console.log("Join succeed.");

(async () => {
    while (true) {
        let msg = await receiver.recv(null);
        console.log(msg.bytesMessage);

        let region = msg.memoryRegions[0];
        if (region) {
            console.log(region.map(0, -1));
        }
    }

    receiver.close();
})();

(async () => {
    while (true) {
        await new Promise((resolve) => {
            setTimeout(() => {
                resolve(null);
            }, 2000);
        });

        sender.send({ labelOp: new LabelOp("a"), mode: SelectorMode.Unicast, ttl: 0 }, { format: 3, data: Buffer.alloc(8) }, []);
    }
})();
