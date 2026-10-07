# D02 reference investigation

Date: 2026-10-06 (Tokyo). Scope: primary documentation and unexecuted free source inspection; no WordPress/container interaction or paid functionality test.

Source: [PublishPress Statuses directory](https://wordpress.org/plugins/publishpress-statuses/), [versioned download](https://downloads.wordpress.org/plugin/publishpress-statuses.1.3.6.zip). Main header and stable tag both report 1.3.6; GPLv3 source remains in the ignored isolated reference directory and is not copied into wpalt. Download: 1,285,355 bytes; SHA-256 `c693403845c046989fe4498a0e19d04298586875fb6fb9e92d4db8c62831a40e`.

Inspected `PostSave.php`, `StatusHandler.php` and `REST.php`, alongside official [post-status](https://wordpress.org/documentation/article/post-status/) and [role](https://wordpress.org/documentation/article/roles-and-capabilities/) documentation. The free source separates editing/status enforcement and assigns status capabilities to eligible WordPress roles. Vendor documentation describes a request/review/publication workflow; it is reference behavior, not evidence that wpalt supports every vendor setting or that paid behavior was observed.

Design inference: wpalt should separate workflow decisions from public snapshots, enforce authority server-side on all transitions, and bind decisions to exact current content rather than treating a status label alone as approval. Its relational state, canonical payload/policy binding, work limits and recovery protocol are independent engineering choices. General custom role policy remains a distinct D13 dependency; existing broad editors must not be misrepresented as restricted contributors.

Repeat source/runtime investigation when affected upstream releases or requirements change. Runtime comparison can use a disposable synthetic WordPress/free-plugin installation once a suitable isolated runtime is available; it does not justify modifying owner data or purchasing a premium account.
