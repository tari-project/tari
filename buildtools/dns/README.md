# DNS Update

A script to update DNS records from the hashes file.

## Setup your env

```
cp .env.sample .env
```

- Edit `.env` to include your cloudflare api TOKEN, your ZONE_ID, and the DOMAIN to update.
- Use FILE to set the name of the file to provide records, if not provided then the default `../../meta/hashes.txt` will be used.

## Install deps

```
npm ci
```

## Run

```
npm run update
```

Deletes existing TXT records on the domain, and creates new records from each line in FILE.

## Release process (manual)

- push tag
- build binaries
- replace hashes in file `meta/hashes.txt`
- sign `meta/hashes.txt` with maintainer gpg key and replace sig at `meta/hashes.txt.sig`
- `npm run update` in this folder to update the DNS records to match the txt file
