# Tari Vulnerability Disclosure Policy

_Last Updated: June 5, 2025_

## Introduction

[Tari Labs](https://tarilabs.com) guides the development of the [Tari open source project](https://tari.com). As the
stewards of Tari, we welcome feedback from security researchers and the general public. If you believe you have
discovered a vulnerability, privacy issue, exposed data, or other security issues in Tari software or Tari Labs
infrastructure, we want to hear from you. This policy outlines steps for reporting vulnerabilities to us, what we
expect, and what you can expect from us.

![hacking_tari](./meta/img/hacker1.webp)

## Scope

This policy applies to:

* Code implementation as seen in the `tari-project` [GitHub repositories](https://github.com/tari-project). This
  includes code in the `development` branches and any release branch.
* Written research from Tari Labs which dictates the above-referenced Tari code implementation. This
  includes [Tari RFCs](https://github.com/tari-project/rfcs) and any published academic or journal articles.
* Infrastructure owned, operated, or maintained by Tari Labs. This includes websites and public-facing applications.

### Out of Scope

This policy does not apply to:

* Archived repositories;
* End-user documentation and educational or "help" materials (e.g., [Tari Labs University](https://tlu.tarilabs.com/));
* Example or test code.
* Proof-of-concept or demonstration applications.
* Code clearly marked as "NOT READY FOR PRODUCTION", "HEAVILY UNDER DEVELOPMENT", or similar wording. An exception is code that is marked as such, 
  but is demonstrably used in production applications covered by the scope anyway.
* Support, marketing, and social media channels (e.g., Telegram or Discord);
* Social engineering of Tari users and Tari Labs staff or contractors;
* Yat vulnerabilities (which should be disclosed via the [Yat Bug Bounty](https://bugcrowd.com/yat-og) program);
* Services, applications, and mix networks run by volunteers (e.g., Tor hidden services);
* Attacks which require more than 50% of network hash rate (or equivalent luck for enough blocks to execute);
* Assets, equipment, and systems not owned by Tari Labs; and
* [Commonplace Reports](#commonplace-reports) as described in this policy.

Vulnerabilities discovered or suspected in systems not owned by Tari Labs should be reported to the appropriate owner,
vendor, or applicable authority. We emphasize that researchers should not engage in Denial of Service, active exploits
against networks, or any physical or electronic attempts against property and/or data centers.

## Disclosure

Please report any **non-sensitive** issue **unrelated to security** as a Github issue in the relevant repository.

Security issues can be disclosed using one of the following channels, in decreasing order of preference: 
  * [HackerOne Bug Bounty Programme](https://hackerone.com/tari_labs)
  * A [Private Security Disclosure](https://github.com/tari-project/tari/security/advisories/new) on Github
  * [The Tari Labs Security mailing list](mailto:security@tari.com) 


Please follow the [requirements](#submission-requirements) below when making your submission.

## Our Commitments

When disclosing a security issue according to this policy, you can expect us to:

* Respond to your report promptly, and work with you to understand and validate your report;
* Let you know if your report qualifies for a bounty reward;
* Strive to keep you informed about the progress of a vulnerability as it is processed;
* Work to remediate discovered vulnerabilities in a timely manner, within our operational constraints;
* Extend [Safe Harbor](#safe-harbor) for your vulnerability research that is related to this policy; and

If your report qualifies for a bounty reward, we will:

* Set a risk level of severity and the reward size;
* Notify you once an issue has been resolved; and
* Provide a time window for the lifting of restrictions around public disclosure.

Disclosures made through the [HackerOne bounty programme](https://hackerone.com/tari_labs) will be acknowledged 
and triaged faster than via the other channels, typically within 7 days. Disclosures made via GitHub or email may 
take longer. 

## Our Expectations

When disclosing a security issue, we ask that you:

* Play by the rules, including following this policy and any other relevant agreements. If there is any inconsistency
  between this policy and any other applicable terms, the terms of this policy will prevail;
* Report any vulnerability you’ve discovered promptly and in good faith;
* Avoid violating the privacy of others, disrupting our systems, destroying data, and/or harming user experience;
* Use only the [Official Channels](#official-channels) to discuss vulnerability information with us;
* Provide us a reasonable amount of time (**at least 60 days** from the initial report) to resolve the issue before you
  disclose it publicly;
* Perform testing only on in-scope systems, and respect systems and activities which are [Out of Scope](#out-of-scope);
* You should only interact with test accounts you own or with explicit permission from the account holder; and
* Do not engage in extortion.

If a vulnerability provides unintended access to data, we ask that you:

* Limit the amount of data you access to the minimum required for effectively demonstrating a proof-of-concept (PoC);
  and
* Cease testing and submit a report immediately if you encounter any user data during testing, such as Personally
  Identifiable Information (PII), Personal Healthcare Information (PHI), credit card data, or proprietary information.

## Safe Harbor

We consider research conducted under this policy to be:

* Authorized concerning any applicable anti-hacking laws, and we will not initiate or support legal action against you
  for accidental, good-faith violations of this policy;

* Authorized concerning any relevant anti-circumvention laws, and we will not bring a claim against you for
  circumvention of technology controls;

* Exempt from restrictions in our terms of service and/or usage policies that would interfere with conducting security
  research, and we waive those restrictions on a limited basis; and

* Lawful, helpful to the overall security of the Internet, and conducted in good faith.

You are expected, as always, to comply with all applicable laws. If legal action is initiated by a third party against
you, and you have complied with this policy, we will take steps to make it known that your actions were conducted in
compliance with this policy.

If at any time you have concerns or are uncertain whether your security research is consistent with this policy, please
submit a report through one of our [Disclosure Channels](#disclosure) before going any further.

* **IMPORTANT:** Please note that Safe Harbor applies only to legal claims under the control of the organization
  participating in this policy, and that the policy does not bind independent third parties.

## Bounty Rewards

**Note:** Multiple vulnerabilities caused by one underlying issue will be awarded one bounty.

### Token-based bounties

It is preferred, but it is not an absolute requirement to make use of the HackerOne bounty programme to claim 
Minotari token rewards.

| Severity | Bounty Range\*      | 
|----------|---------------------|
| Critical | 100,000 XTM - 250,000 XTM | 
| High     | 25,000 XTM - 75,000 XTM | 
| Medium   | 5,000 - 15,000  XTM  | 
| Low      | 500 - 5,000 XTM      | 

#### Notes and conditions for token bounty rewards:

* Minotari earned through the bounty programme will unencumbered and can be spent or traded immediately after
  conversion.
* Researchers will have to provide a valid Tari wallet emoji id in order to receive their Minotari tokens. Tari Labs
  will not custody any tokens on behalf of researchers.
* Tari Labs reserves the right to adjust the bounty reward amounts from time to time. 

Our rewards are based on severity per CVSS (the Common Vulnerability Scoring Standard) as determined by Tari Labs. 
Please note these are general guidelines, and reward decisions are up to the discretion of Tari Labs.

Please allow up to one week from the time the report was approved and validated to receive your bounty reward payment.

### Submission Requirements

Please adhere to the following requirements when making a submission.

1. **Report format:** For any report to be considered, you must provide clear instructions on how to reproduce the 
   issue,
  as well as how it could be exploited (i.e., attack scenario), and what you think the security impact is. This
  information helps us assess eligibility and appropriate bounty reward amounts. 
  The following format can be used as 
   a rough, outline:
    * A description of the issue;
    * A proof-of-concept (PoC) or steps you took to create the issue; 
    * Screenshots and/or a video demonstration.
  
2. Whenever possible, please include:
    * Affected software versions; and
    * If known, mitigations for the issue.
The more detail you provide, the easier it will be for us to triage and fix the issue.

3. **Submit one vulnerability per report**, unless you need to chain vulnerabilities to provide impact.

4. **First come, first served:** Only the first person to identify a particular vulnerability will qualify for a bounty
  reward. Any additional reports will be considered as duplicates and will not qualify.

5. **Play it safe:** All testing must be performed on test accounts under your control. Any attacks against other users
  without provable express consent are not allowed. If a particular issue is severe enough that
  a proof-of-concept (PoC) in itself may expose sensitive data (e.g., data of other users), please ask us for help first
  so we can work together on how to safely demonstrate the bug.

6. **Don't disclose too early:** To protect our users, please keep all identified vulnerability details between you and
  us until we've had a chance to fix the issue. This includes things like posting an obscured video of an issue on
  social media prior to confirmation of a deployed fix. Though you may think you have concealed critical details, doing
  so at minimum alerts potentially malicious actors that an issue exists and at worst unintentionally creates early
  disclosure. 

7. **No social engineering:** Bugs that require social engineering to exploit (e.g., tricking someone into clicking a
  link) may qualify, but please do not actually attempt to socially engineer another user, Tari Labs staff, Tari open
  source volunteers, etc. during your testing. Providing a clear explanation of how social engineering could be used in
  conjunction with an identified vulnerability is sufficient.

### Commonplace Reports

In addition to the areas defined as [Out of Scope](#out-of-scope) in this policy, the following 
non-exhaustive commonplace reports do not qualify for a bounty reward. Such issues may be disclosed as
a [GitHub issue here](https://github.com/tari-project/tari/issues/new?assignees=&labels=bug-report&template=bug_report.md&title=%5BThanks%20for%20making%20Tari%20better%5D).

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
* Reports resulting from automated scanning utilities without additional details or a PoC demonstrating a specific
  exploit
* Attacks requiring physical access to a user’s device
* Attacks dependent upon social engineering of Tari Labs staff, contractors, or vendors
* Attacks dependent upon social engineering of Tari open source volunteers
* Attacks dependent upon social engineering of Tari users
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
                              
Thank you for helping keep Tari and our users safe!
