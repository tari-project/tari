# Tari developer public keys

This folder contains the public keys for Tari developers

## Why use public keys

Here's a
[long, but interesting read](https://mikegerwitz.com/2012/05/a-git-horror-story-repository-integrity-with-signed-commits).

## Creating a public key

If you don't already have a GPG key, follow the steps
[described here](https://help.github.com/articles/generating-a-new-gpg-key/) to create one.

## Importing keys into your keyring

Lot of detail on how to import keys into your keyring is given in the
[Redhat docs](https://access.redhat.com/documentation/en-US/Red_Hat_Enterprise_Linux/4/html/Step_by_Step_Guide/s1-gnupg-import.html),
but here is a tl;dr:

    gpg --import key.asc 

The output looks similar to the following:

```text
gpg: key F78FFE84: public key imported
gpg: Total number processed: 1
gpg:               imported: 1
```

## Signing commits with your key

https://help.github.com/articles/signing-commits/

## Submitting your public key

To add your GPG key to the list, export your **public**** key with

    gpg --armor --export <your_email_address>_

thne, create a pull request with your GPG public key in a single file in this folder with the name
`<your-github-handle>.asc`