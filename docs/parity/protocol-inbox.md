# Pending protocol links

Production source qualification, 2026-10-02. Runtime and native platform acceptance remain deferred.

The UI retains Modsync requests in a FIFO and retains the existing NXM FIFO. Their combined pending capacity is 64 links; a request already owned by an active worker is separate. The transport channels retain their existing bounded capacity. Every transport receive first checks pending capacity, so acknowledged requests remain in their transport channel while the UI is full. The UI handles at most 64 messages per frame, including activation messages. On macOS it alternates preference between native and local transports, falling back to the other when the preferred one is empty. Arrival order within each transport is preserved; there is no claimed total order between independent transports or between Modsync and NXM execution.

Requests use the shared 1–8192 byte qualification before parsing. Rejection logs do not echo the authored URL. Only the first pending Modsync request can be opened or dismissed, and a worker completion or failure leaves later requests queued. Prompt URLs use the existing secret redaction.

Closing stops intake and starting work. The close confirmation describes discarded pending links. These queues are in memory, not durable across closing, crashes, or restarts. When capacity blocks receiving, an activation message behind retained links waits as well.
