async function waitForInit() {
    while (!Level.initialized) {
        await Level.tick();
    }
}

// Simple process subscription loop
(async function () {
    await waitForInit();

    while (true) {
        try {
            Level.processSubscription();
        } catch (e) {
            console.error(e);
        }

        await Level.tick();
    }
})();

(async function () {
    await waitForInit();

    const id = Level.registerChannel(
        "test",
        {
            publish: true,
            subscribe: true,
        },
        (msg) => console.log(`Received message: ${msg}`),
    );
    let t = 0;

    while (true) {
        const { x, y, z } = Level.getBlockEntity(Level.uuid);
        let cmd;
        if (x == 0 && z != Chunk.chunkSize - 1) {
            cmd = {
                command: "move",
                direction: "forward",
            };
        } else if (x == Chunk.chunkSize - 1 && z != 0) {
            cmd = {
                command: "move",
                direction: "backward",
            };
        } else if (z == 0) {
            cmd = {
                command: "move",
                direction: "right",
            };
        } else {
            cmd = {
                command: "move",
                direction: "left",
            };
        }

        Level.publishChannel(id, `Message: ${t}`);
        t += 1;
        await Level.submit(cmd);
    }
})();
