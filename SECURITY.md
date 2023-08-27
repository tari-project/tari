# Taiji Vulnerability Disclosure Policy
_Last Updated: April 24, 2023_

## Introduction

[Taiji Labs](https://taijilabs.com) guides the development of the [Taiji open source project](https://taiji.com). As the stewards of Taiji, we welcome feedback from security researchers and the general public. If you believe you have discovered a vulnerability, privacy issue, exposed data, or other security issues in Taiji software or Taiji Labs infrastructure, we want to hear from you. This policy outlines steps for reporting vulnerabilities to us, what we expect, and what you can expect from us.

## Scope

This policy applies to:

* Code implementation as seen in the `taiji-project` [GitHub repositories](https://github.com/taiji-project). This includes code in the `development` branches and any release branch.
* Written research from Taiji Labs which dictates the above-referenced Taiji code implementation. This includes [Taiji RFCs](https://github.com/taiji-project/rfcs) and any published academic or journal articles.
* Infrastructure owned, operated, or maintained by Taiji Labs. This includes websites and public-facing applications.

### Out of Scope

This policy does not apply to:

* Archived repositories;
* End-user documentation and educational or "help" materials (e.g., [Taiji Labs University](https://tlu.taijilabs.com/));
* Support, marketing, and social media channels (e.g., Telegram or Discord);
* Social engineering of Taiji users and Taiji Labs staff or contractors;
* Yat vulnerabilities (which should be disclosed via the [Yat Bug Bounty](https://bugcrowd.com/yat-og) program);
* Services, applications, and mix networks run by volunteers (e.g., Tor hidden services);
* Attacks which require more than 50% of network hash rate (or equivalent luck for enough blocks to execute);
* Assets, equipment, and systems not owned by Taiji Labs; and
* [Commonplace Reports](#commonplace-reports) as described in this policy.

Vulnerabilities discovered or suspected in systems not owned by Taiji Labs should be reported to the appropriate owner, vendor, or applicable authority. We emphasize that researchers should not engage in Denial of Service, active exploits against networks, or any physical or electronic attempts against property and/or data centers.

## Disclosure

Please report any **non-sensitive issue unrelated to security** as a [GitHub issue here](https://github.com/taiji-project/taiji/issues/new?assignees=&labels=bug-report&template=bug_report.md&title=%5BThanks%20for%20making%20Taiji%20better%5D). Thank you for helping to make Taiji more robust and reliable.

* [Taiji Issues](https://github.com/taiji-project/taiji/issues)
* [iOS Issues](https://github.com/taiji-project/wallet-ios/issues)
* [RFC Docs](https://rfc.taiji.com/)
* [Contributing Guidelines](https://github.com/taiji-project/taiji/blob/55481043b99ef0381289e9ac85973dc1b603ba81/Contributing.md)

### Security Issues

The following information applies to vulnerabilities, exploits, or undesirable behavior that is too sensitive for public disclosure. In these cases, we appreciate private, responsible disclosure by amateurs and experts alike. We would like to thank you in advance for your adherence to professional standards of conduct.

#### Official Channels

Please report security issues to [professor@taiji.com](mailto:professor@taiji.com), providing all relevant information.

```
professor@taiji.com
PGP fingerprint = 5410 7BD9 02F0 A865 3DDF F4CD 7A4A 432E C35C 9C7E

If pasting GPG encrypted data, use paste.debian.net or paste.ubuntu.com
as these do not introduce issues with Tor via Cloudflare.
```

**You must include:**

* A description of the issue;
* A proof-of-concept (PoC) or steps you took to create the issue; and
* Screenshots and/or a video demonstration.

**Whenever possible, please include:**

* Affected software versions; and
* If known, mitigations for the issue.

The more detail you provide, the easier it will be for us to triage and fix the issue. Taiji follows a **60 day disclosure timeline** as described in this policy.

#### Yat Issues

[Yat](https://y.at) vulnerabilities should be disclosed via the [Yat Bug Bounty](https://bugcrowd.com/yat-og) program.

## Our Commitments

When disclosing a security issue according to this policy, you can expect us to:

* Respond to your report promptly, and work with you to understand and validate your report;
* Let you know if your report qualifies for a bounty reward within five business days;
* Strive to keep you informed about the progress of a vulnerability as it is processed;
* Work to remediate discovered vulnerabilities in a timely manner, within our operational constraints;
* Extend [Safe Harbor](#safe-harbor) for your vulnerability research that is related to this policy; and

If your report qualifies for a bounty reward, we will:

* Set a risk level of severity and the reward size within five business days;
* Resolve qualifying vulnerabilities within 60 days (1 day for critical, 1-2 weeks for high, 4-8 weeks for medium, and 60 days for low issues);
* Notify you once an issue has been resolved; and
* Provide a time window for the lifting of restrictions around public disclosure.

## Our Expectations

When disclosing a security issue, we ask that you:

* Play by the rules, including following this policy and any other relevant agreements. If there is any inconsistency between this policy and any other applicable terms, the terms of this policy will prevail;
* Report any vulnerability you’ve discovered promptly and in good faith;
* Avoid violating the privacy of others, disrupting our systems, destroying data, and/or harming user experience;
* Use only the [Official Channels](#official-channels) to discuss vulnerability information with us;
* Provide us a reasonable amount of time (**at least 60 days** from the initial report) to resolve the issue before you disclose it publicly;
* Perform testing only on in-scope systems, and respect systems and activities which are [Out of Scope](#out-of-scope);
* You should only interact with test accounts you own or with explicit permission from the account holder; and
* Do not engage in extortion.

If a vulnerability provides unintended access to data, we ask that you:

* Limit the amount of data you access to the minimum required for effectively demonstrating a proof-of-concept (PoC); and
* Cease testing and submit a report immediately if you encounter any user data during testing, such as Personally Identifiable Information (PII), Personal Healthcare Information (PHI), credit card data, or proprietary information.

## Safe Harbor

We consider research conducted under this policy to be:

* Authorized concerning any applicable anti-hacking laws, and we will not initiate or support legal action against you for accidental, good-faith violations of this policy;

* Authorized concerning any relevant anti-circumvention laws, and we will not bring a claim against you for circumvention of technology controls;

* Exempt from restrictions in our terms of service and/or usage policies that would interfere with conducting security research, and we waive those restrictions on a limited basis; and

* Lawful, helpful to the overall security of the Internet, and conducted in good faith.

You are expected, as always, to comply with all applicable laws. If legal action is initiated by a third party against you and you have complied with this policy, we will take steps to make it known that your actions were conducted in compliance with this policy.

If at any time you have concerns or are uncertain whether your security research is consistent with this policy, please submit a report through one of our [Official Channels](#official-channels) before going any further.

* **IMPORTANT:** Please note that Safe Harbor applies only to legal claims under the control of the organization participating in this policy, and that the policy does not bind independent third parties.

## Bounty Rewards

The value of rewards paid out varies depending on severity and will be guided by the rules in this policy. Rewards  are payable in USD or, optionally, an equivalent amount in cryptocurrency may be requested. If you prefer, you may also elect to have your reward donated to a registered charity of your choice that accepts online donations, subject to approval of the charity.

### Payment Amounts

* **Medium, Large, or Critical:** Between $120 to $5000 USD
* **Small:** Up to $100 USD

Please allow up to one week from the time the report was approved and validated to receive your bounty reward payment.

### Eligibility
The following requirements must be adhered to in order to for any report to qualify for a bounty reward. Not following these requirements can result in your report being rejected or the banning of your further submissions.

* **Report format:** For any report to be considered, you must provide clear instructions on how to reproduce the issue, as well as how it could be exploited (i.e., attack scenario), and what you think the security impact is. This information helps us assess eligibility and appropriate bounty reward amounts.

* **First come, first served:** Only the first person to identify a particular vulnerability will qualify for a bounty reward. Any additional reports will be considered as duplicates and will not qualify.

* **Play it safe:** All testing must be performed on test accounts under your control. Any attacks against other users without provable express consent are not allowed and may result in a ban. If a particular issue is severe enough that a proof-of-concept (PoC) in itself may expose sensitive data (e.g., data of other users), please ask us for help first so we can work together on how to safely demonstrate the bug.

* **Don't disclose too early:** To protect our users, please keep all identified vulnerability details between you and us until we've had a chance to fix the issue. This includes things like posting an obscured video of an issue on social media prior to confirmation of a deployed fix. Though you may think you have concealed critical details, doing so at minimum alerts potentially malicious actors that an issue exists and at worst unintentionally creates early disclosure. Public disclosure prior to us notifying you of the fix may result in a ban. If you have questions regarding the remediation timeline, please inquire on the relevant report.

* **No social engineering:** Bugs that require social engineering to exploit (e.g., tricking someone into clicking a link) may qualify, but please do not actually attempt to socially engineer another user, Taiji Labs staff, Taiji open source volunteers, etc. during your testing. Providing a clear explanation of how social engineering could be used in conjunction with an identified vulnerability is sufficient.

### Commonplace Reports

In addition to the areas defined as [Out of Scope](#out-of-scope) in this policy, the following commonplace reports do not qualify for a bounty reward. Such issues may be disclosed as a [GitHub issue here](https://github.com/taiji-project/taiji/issues/new?assignees=&labels=bug-report&template=bug_report.md&title=%5BThanks%20for%20making%20Taiji%20better%5D).

* Lack of a security feature that is not critical to the system's operation
* Configuration issues that are not relevant to the network or application
* Application Denial of Service by locking user accounts
* Descriptive error messages or headers (e.g., stack traces, banner grabbing, debug information on a production site)
* Purely technical, public, and non-sensitive network, application, or API information unrelated to a specific exploit
* Disclosure of known public files or directories, (e.g., `robots.txt`)
* Outdated software/library versions
* Lack of security headers, such as the `X-Content-Type-Options` or `X-Frame-Options` headers
* OPTIONS/TRACE HTTP method enabled
* Subdomain takeover, such as a subdomain pointing to a service that is no longer in use
* DNS Zone transfer and configuration issues
* CSRF on logout
* CSRF on forms that are available to anonymous users
* Cookies with missing or incomplete flags such as `HTTP Only` or `Secure`
* Self-XSS and issues exploitable only through Self-XSS
* XSS that does not allow for the execution of arbitrary code, such as reflected or non-persistent XSS
* Reports resulting from automated scanning utilities without additional details or a PoC demonstrating a specific exploit
* Attacks requiring physical access to a user’s device
* Attacks dependent upon social engineering of Taiji Labs staff, contractors, or vendors
* Attacks dependent upon social engineering of Taiji open source volunteers
* Attacks dependent upon social engineering of Taiji users
* Username enumeration based on login or "forgot password" pages
* Enforcement policies for brute force, rate limiting, or account lockout
* SSL/TLS best practices
* SSL/TLS attacks such as BEAST, BREACH, Renegotiation attack
* Clickjacking, without additional details demonstrating a specific exploit
* Mail configuration issues including SPF, DKIM, DMARC settings
* Use of a known-vulnerable library without a description of an exploit specific to our implementation
* Password and account recovery policies
* Presence of autocomplete functionality in form fields
* Publicly-accessible login panels
* Lack of email address verification during account registration or account invitation
* Lack of email address verification password restore
* Session control during email/password changes
