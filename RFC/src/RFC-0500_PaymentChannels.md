# RFC-0500: Tari payment channels

RFC placeholder.

This document will describe how Tari implements payment channels. The base layer is slow. The DAN is fast. However,
every non-read DAN instruction has a fee (i.e. base layer transaction) associated with it. To bridge the speed gap
between the two layers, a payment channel solution is required. This document provides the high-level overview of how
this is done.