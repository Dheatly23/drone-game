
(async function () {
    // Wait for initialization
    while (!Level.initialized) {
        await Level.tick();
    }

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

        await Level.submit(cmd);
    }
})()
