# The Tari RFCs

Tari is a community-driven project. The documents presented in this RFC collection have typically gone through several
iterations before reaching this point:

* Ideas and questions are posted in #tari-dev on [#FreeNode IRC](https://freenode.net/). This is typically short-form
  content with rapid feedback. Often, these conversations will lead to someone posting an [issue] or RFC [pull request].
* RFCs are "Requests for Comment", so although the proposals in these documents are usually well-thought out, they are
  not cast in stone. RFCs can, and should, undergo further evaluation and discussion by the community. RFC comments are
  best made using Github [issue]s.

New RFC's should follow the format given in the [RFC template](RFC_template.md).
## Lifecycle

RFCs go through the following lifecycle, which roughly corresponds to the [COSS](https://github.com/unprotocols/rfc/blob/master/2/README.md):

| Status      |                                                   | Description                                                                                                                                                                                                         |
|:------------|:--------------------------------------------------|:--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Draft       | ![draft](theme/images/status-draft.svg)           | Changes, additions and revisions can be expected.                                                                                                                                                                   |
| Stable      | ![stable](theme/images/status-stable.svg)         | Typographical and cosmetic changes aside, no further changes should be made. Changes to the Tari code base w.r.t. a stable RFC will lead to the RFC becoming out of date, deprecated, or retired.                   |
| Out of date | ![out of date](theme/images/status-outofdate.svg) | This RFC has become stale due to changes in the code base. Contributions will be accepted to make it stable again if the changes are relatively minor, otherwise it should eventually become deprecated or retired. |
| Deprecated  | ![deprecated](theme/images/status-deprecated.svg) | This RFC has been replaced by a newer RFC document, but is still is use in some places and/or versions of Tari.                                                                                                     |
| Retired     | ![retired](theme/images/status-retired.svg)       | The RFC is no longer in use on the Tari network.                                                                                                                                                                    |


[pull request]: https://github.com/tari-project/tari/pulls?q=is%3Aopen+is%3Apr+label%3ARFC 'Tari RFC pull requests'
[issue]: https://github.com/tari-project/tari/issues?q=is%3Aissue+label%3ARFC 'Tari RFC Issues'