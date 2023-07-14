# ipmb-js

## Usage

```shell
npm i [@<scope>/]ipmb-js
```

```js
const { join, SelectorMode } = require('[@<scope>/]ipmb-js');

let { sender, receiver } = join({
    identifier: 'solar.com',
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

sender.send({ label: ["moon"], mode: SelectorMode.Unicast, ttl: 0 }, { format: 0, data: Buffer.alloc(8) }, []);

```