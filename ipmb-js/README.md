## ipmb-js

The `ipmb-js` package is Node.js binding for [ipmb](https://github.com/bytedance/ipmb), 
an interprocess message bus system built in Rust.

## Usage

```js
const { join, LabelOp, SelectorMode } = require('ipmb-js');

let { sender, receiver } = join({
    identifier: 'com.solar',
    label: ['earth'],
    token: '',
    controllerAffinity: true,
}, null);

(async () => {
    while (true) {
        let msg = await receiver.recv(null);
        console.log(msg.bytesMessage);

        let region = msg.memoryRegions[0];
        if (region) {
            // Map the memory region from 0 to end
            console.log(region.map(0, -1));
        }
    }
})()

let selector = { 
    label: new LabelOp("moon"), 
    mode: SelectorMode.Unicast, 
    ttl: 0 
};
    
let bytesMessage = { format: 0, data: Buffer.alloc(8) };

sender.send(selector, bytesMessage, []);

```