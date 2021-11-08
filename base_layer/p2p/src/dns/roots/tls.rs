//! This library is automatically generated from the Mozilla certificate
//! store via mkcert.org.  Don't edit it.
//!
//! The generation is done deterministically so you can verify it
//! yourself by inspecting and re-running the generation process.

#![forbid(unsafe_code, unstable_features)]
#![deny(
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_extern_crates,
    unused_qualifications
)]

pub static TLS_SERVER_ROOTS: webpki::TLSServerTrustAnchors = webpki::TLSServerTrustAnchors(&[
    /*
     * Issuer: CN=Entrust Root Certification Authority - EC1 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2012 Entrust, Inc. - for authorized use only
     * Subject: CN=Entrust Root Certification Authority - EC1 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2012 Entrust, Inc. - for authorized use only
     * Label: "Entrust Root Certification Authority - EC1"
     * Serial: 51543124481930649114116133369
     * MD5 Fingerprint: b6:7e:1d:f0:58:c5:49:6c:24:3b:3d:ed:98:18:ed:bc
     * SHA1 Fingerprint: 20:d8:06:40:df:9b:25:f5:12:25:3a:11:ea:f7:59:8a:eb:14:b5:47
     * SHA256 Fingerprint: 02:ed:0e:b2:8c:14:da:45:16:5c:56:67:91:70:0d:64:51:d7:fb:56:f0:b2:ab:1d:3b:8e:b0:70:e5:6e:df:f5
     * -----BEGIN CERTIFICATE-----
     * MIIC+TCCAoCgAwIBAgINAKaLeSkAAAAAUNCR+TAKBggqhkjOPQQDAzCBvzELMAkG
     * A1UEBhMCVVMxFjAUBgNVBAoTDUVudHJ1c3QsIEluYy4xKDAmBgNVBAsTH1NlZSB3
     * d3cuZW50cnVzdC5uZXQvbGVnYWwtdGVybXMxOTA3BgNVBAsTMChjKSAyMDEyIEVu
     * dHJ1c3QsIEluYy4gLSBmb3IgYXV0aG9yaXplZCB1c2Ugb25seTEzMDEGA1UEAxMq
     * RW50cnVzdCBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5IC0gRUMxMB4XDTEy
     * MTIxODE1MjUzNloXDTM3MTIxODE1NTUzNlowgb8xCzAJBgNVBAYTAlVTMRYwFAYD
     * VQQKEw1FbnRydXN0LCBJbmMuMSgwJgYDVQQLEx9TZWUgd3d3LmVudHJ1c3QubmV0
     * L2xlZ2FsLXRlcm1zMTkwNwYDVQQLEzAoYykgMjAxMiBFbnRydXN0LCBJbmMuIC0g
     * Zm9yIGF1dGhvcml6ZWQgdXNlIG9ubHkxMzAxBgNVBAMTKkVudHJ1c3QgUm9vdCBD
     * ZXJ0aWZpY2F0aW9uIEF1dGhvcml0eSAtIEVDMTB2MBAGByqGSM49AgEGBSuBBAAi
     * A2IABIQTydC6bUF74mzQ61VfZgIaJPRbiWlH47jCffHyAsWfoPZb1YsGGYZPUxBt
     * ByQnoaD41UcZYUx9ypMn6nQM72+WCf5j7HBdNq1nd67JnXxVRDqiY1Ef9eNi1KlH
     * Bz7MIKNCMEAwDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0O
     * BBYEFLdj5xrdjekIplWDpOBqUEFlEUJJMAoGCCqGSM49BAMDA2cAMGQCMGF52OVC
     * R98crlOZF7ZvHH3hvxGU0QOIdeSNiaSKd0bebWHvAvX7td/M/k7//qnmpwIwW5nX
     * hTcGtXsI/esni0qU+eH6p44mCOh8kmhtc9hvJqwhAriZtyZBWyVgrtBIGu4G
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x160\x14\x06\x03U\x04\n\x13\rEntrust, Inc.1(0&\x06\x03U\x04\x0b\x13\x1fSee www.entrust.net/legal-terms1907\x06\x03U\x04\x0b\x130(c) 2012 Entrust, Inc. - for authorized use only1301\x06\x03U\x04\x03\x13*Entrust Root Certification Authority - EC1",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x84\x13\xc9\xd0\xbamA{\xe2l\xd0\xebU_f\x02\x1a$\xf4[\x89iG\xe3\xb8\xc2}\xf1\xf2\x02\xc5\x9f\xa0\xf6[\xd5\x8b\x06\x19\x86OS\x10m\x07$\'\xa1\xa0\xf8\xd5G\x19aL}\xca\x93\'\xeat\x0c\xefo\x96\t\xfec\xecp]6\xadgw\xae\xc9\x9d|UD:\xa2cQ\x1f\xf5\xe3b\xd4\xa9G\x07>\xcc ",
        name_constraints: None
    },

    /*
     * Issuer: CN=AffirmTrust Commercial O=AffirmTrust
     * Subject: CN=AffirmTrust Commercial O=AffirmTrust
     * Label: "AffirmTrust Commercial"
     * Serial: 8608355977964138876
     * MD5 Fingerprint: 82:92:ba:5b:ef:cd:8a:6f:a6:3d:55:f9:84:f6:d6:b7
     * SHA1 Fingerprint: f9:b5:b6:32:45:5f:9c:be:ec:57:5f:80:dc:e9:6e:2c:c7:b2:78:b7
     * SHA256 Fingerprint: 03:76:ab:1d:54:c5:f9:80:3c:e4:b2:e2:01:a0:ee:7e:ef:7b:57:b6:36:e8:a9:3c:9b:8d:48:60:c9:6f:5f:a7
     * -----BEGIN CERTIFICATE-----
     * MIIDTDCCAjSgAwIBAgIId3cGJyapsXwwDQYJKoZIhvcNAQELBQAwRDELMAkGA1UE
     * BhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MR8wHQYDVQQDDBZBZmZpcm1UcnVz
     * dCBDb21tZXJjaWFsMB4XDTEwMDEyOTE0MDYwNloXDTMwMTIzMTE0MDYwNlowRDEL
     * MAkGA1UEBhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MR8wHQYDVQQDDBZBZmZp
     * cm1UcnVzdCBDb21tZXJjaWFsMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC
     * AQEA9htPZwcroRX1BiLLHwGy43NFBkRJLLtJJRTWzsO3qyxPxkEylFf6EqdbDuKP
     * Hx6GGaeqtS25Xw2Kwq+FNXkyLbscYjfysVtKPcrNcV/pQr6U6Mje+SJIZMblq8Yr
     * ba0F8PrVC8+a5fBQpIs7R6UjW3p6+DM/uO+Zl+MgwdYoic+U+7lF7eNAFxHUdPAL
     * MeIrJmqbTFeurCA+ukV6BfO9m2kVrn1OIGPENXY6BwLJN/3HR+7o8XYdcxXyl6S1
     * yHp52UKqK39c/s4mT6NmgTWvRLpUHhwwMmWd5jyTXlBOeuM61G7MGvv50jeuJCqr
     * VwMiKA1JdX+3KNp1v47j3A55MQIDAQABo0IwQDAdBgNVHQ4EFgQUnZPGU4teyq8/
     * nx4P5ZmVvCT2lI8wDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwDQYJ
     * KoZIhvcNAQELBQADggEBAFis9AQOzcAN/wr91LoWXym9e2iZWEnStB03TX8nfUYG
     * XUPGhi4+c7ImfU+TqbbEKpqrIZcUsd6M06uJFdhrJNTxFq7YpFzUf1GO7RgBsZNj
     * vbz4YYCanrHOQnDiqX0GJX0nof5v7LMeJNrjS1UaADs1tDvZ110w/YETifLCBivt
     * Z8SOyUOyXGsViQK8YvxO8rUzqrJv0wqiUOP2O+guRMLbZjipM1ZI8W0bM40NjD9g
     * N53Tym1+NH4Nn3J2ixufcv1SNUFFApYvHLKac0khsUlHRUe072o0EclNmsxZt9YC
     * nlpOZbWUrhvfKbAW8b8Angc6F2S1BLUjIZkKlTuXfO8=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x140\x12\x06\x03U\x04\n\x0c\x0bAffirmTrust1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16AffirmTrust Commercial",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xf6\x1bOg\x07+\xa1\x15\xf5\x06\"\xcb\x1f\x01\xb2\xe3sE\x06DI,\xbbI%\x14\xd6\xce\xc3\xb7\xab,O\xc6A2\x94W\xfa\x12\xa7[\x0e\xe2\x8f\x1f\x1e\x86\x19\xa7\xaa\xb5-\xb9_\r\x8a\xc2\xaf\x855y2-\xbb\x1cb7\xf2\xb1[J=\xca\xcdq_\xe9B\xbe\x94\xe8\xc8\xde\xf9\"Hd\xc6\xe5\xab\xc6+m\xad\x05\xf0\xfa\xd5\x0b\xcf\x9a\xe5\xf0P\xa4\x8b;G\xa5#[zz\xf83?\xb8\xef\x99\x97\xe3 \xc1\xd6(\x89\xcf\x94\xfb\xb9E\xed\xe3@\x17\x11\xd4t\xf0\x0b1\xe2+&j\x9bLW\xae\xac >\xbaEz\x05\xf3\xbd\x9bi\x15\xae}N c\xc45v:\x07\x02\xc97\xfd\xc7G\xee\xe8\xf1v\x1ds\x15\xf2\x97\xa4\xb5\xc8zy\xd9B\xaa+\x7f\\\xfe\xce&O\xa3f\x815\xafD\xbaT\x1e\x1c02e\x9d\xe6<\x93^PNz\xe3:\xd4n\xcc\x1a\xfb\xf9\xd27\xae$*\xabW\x03\"(\rIu\x7f\xb7(\xdau\xbf\x8e\xe3\xdc\x0ey1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Autoridad de Certificacion Firmaprofesional CIF A62634068
     * Subject: CN=Autoridad de Certificacion Firmaprofesional CIF A62634068
     * Label: "Autoridad de Certificacion Firmaprofesional CIF A62634068"
     * Serial: 6047274297262753887
     * MD5 Fingerprint: 73:3a:74:7a:ec:bb:a3:96:a6:c2:e4:e2:c8:9b:c0:c3
     * SHA1 Fingerprint: ae:c5:fb:3f:c8:e1:bf:c4:e5:4f:03:07:5a:9a:e8:00:b7:f7:b6:fa
     * SHA256 Fingerprint: 04:04:80:28:bf:1f:28:64:d4:8f:9a:d4:d8:32:94:36:6a:82:88:56:55:3f:3b:14:30:3f:90:14:7f:5d:40:ef
     * -----BEGIN CERTIFICATE-----
     * MIIGFDCCA/ygAwIBAgIIU+w77vuySF8wDQYJKoZIhvcNAQEFBQAwUTELMAkGA1UE
     * BhMCRVMxQjBABgNVBAMMOUF1dG9yaWRhZCBkZSBDZXJ0aWZpY2FjaW9uIEZpcm1h
     * cHJvZmVzaW9uYWwgQ0lGIEE2MjYzNDA2ODAeFw0wOTA1MjAwODM4MTVaFw0zMDEy
     * MzEwODM4MTVaMFExCzAJBgNVBAYTAkVTMUIwQAYDVQQDDDlBdXRvcmlkYWQgZGUg
     * Q2VydGlmaWNhY2lvbiBGaXJtYXByb2Zlc2lvbmFsIENJRiBBNjI2MzQwNjgwggIi
     * MA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQDKlmuO6vj78aI14H9M2uDDUtd9
     * thDIAl6zQyrET2qyyhxdKJp4ERppWVevtSBC5IsP5t9bpgOSL/UR5GLXMnE42QQM
     * cas9UX4PB99jBVzpv5RvwSmCwLTaUbDBPLutN0pcyvFLNg4kq7/DhHf9qFD0sefG
     * L9ItWY16Ck6WaVICqjaY7Pz6FIMMNx/Jkjd/14Et5cS54D40/mf0PmbR0/RAz15i
     * NA9wBj4gGFrO93IbJWyTdBSTo3OxDqqHECNZXyAFGUftaI6SEspd/NYrspI8IM/h
     * X68gvqB2f3bl7BqGYTM+53u0P6APjqK5am+5hyZvQWyIplD9amML9ZMWGxmPsu2b
     * m8mQ9QEM3xk9Dz44I8kvjwzRAv4bVdZO0I08r0+k8/6vKtMFnXkIoctXMbScyJCy
     * Z/QYFpM6/EfY0XiWMR+6KwxfXZmtY4laJCB22N/9q06mIqqdXuYnin1oKaPnirja
     * EbsXLZmdEyRG98Xi2J+Of8ePdG1asuhy9azuJBCtLxTa/y2aRnFHvkLfuwHb9H/T
     * KI8xWVvTyQKmtFLKbpf7Q8UIJm+K9Lv9nyiqDdVF8xM6HdjAeI9BZzwelGSuewvF
     * 6NkBiDkal4ZkQdU7hwxu+g/GvUgUvzlN1J5Bto+WHWOWk9mVBngxaJ43BjuAiUVh
     * OSPHG0SjFeUc+JIwuwIDAQABo4HvMIHsMBIGA1UdEwEB/wQIMAYBAf8CAQEwDgYD
     * VR0PAQH/BAQDAgEGMB0GA1UdDgQWBBRlzeurNR4APn7VdMActHNHDhpkLzCBpgYD
     * VR0gBIGeMIGbMIGYBgRVHSAAMIGPMC8GCCsGAQUFBwIBFiNodHRwOi8vd3d3LmZp
     * cm1hcHJvZmVzaW9uYWwuY29tL2NwczBcBggrBgEFBQcCAjBQHk4AUABhAHMAZQBv
     * ACAAZABlACAAbABhACAAQgBvAG4AYQBuAG8AdgBhACAANAA3ACAAQgBhAHIAYwBl
     * AGwAbwBuAGEAIAAwADgAMAAxADcwDQYJKoZIhvcNAQEFBQADggIBABd9oPm03cXF
     * 661LJLWhAqvdpYhKsg9VSytXjDvlMd3+xDLx51tkljYyGOylMnfX40S2wBEqgLk9
     * am58m9Ot/MPWo+ZkKXzR4Tgegiv/J2Wv+xYVxC5xhOW1//qkR71kMrv2JYSiJ0L1
     * ILDCExARzRAVukKQKtJE4ZYm6zFIEv0q2skGz3QeqUvVhyj5eTSSPi5E6PaPT481
     * PyWzOdxjKpBrIF/EUhJOlywqrJ2X3kjyo2bbwtKDlaZmp54lD+kLM5FlClrD2VQS
     * 3a/DTg4fJl4N3LON7NWBcN7STyQF82xO9UxJZo3R/9ILJUFI/lGExkKvgATP0H5k
     * SeTy36LssUzAKh3ntLFlosS88Zj0qnAHY7S42jtM+kAiMFsRpvAFDsYCA0irhpuF
     * 3dvd6qJ2gHN99ZwExEWN57kci57q13XRcrHedUTnQn3iV2t93Jm8PYMo6oCTjcVM
     * ZcFwgbg4/EMxsvYDNEeyrPsiBsse3RdHHF9mudMaotoRsaS8I8nkvof/uZS2+F0g
     * StRf571oe2XyFR7SOqkt6dhrJKyXWERHrVkY8SFlcN7ONGCoQPHzPKTDKCOM/icz
     * Q0CgFzzr6juwcqajuUpLXhZI9LK8yIySxZ2frHI2vDSANGupi5LAuBft7HZT9SQB
     * jLMi6Et8Vcad+qMUu2WFbm5PEn4KPJ2V
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02ES1B0@\x06\x03U\x04\x03\x0c9Autoridad de Certificacion Firmaprofesional CIF A62634068",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xca\x96k\x8e\xea\xf8\xfb\xf1\xa25\xe0\x7fL\xda\xe0\xc3R\xd7}\xb6\x10\xc8\x02^\xb3C*\xc4Oj\xb2\xca\x1c](\x9ax\x11\x1aiYW\xaf\xb5 B\xe4\x8b\x0f\xe6\xdf[\xa6\x03\x92/\xf5\x11\xe4b\xd72q8\xd9\x04\x0cq\xab=Q~\x0f\x07\xdfc\x05\\\xe9\xbf\x94o\xc1)\x82\xc0\xb4\xdaQ\xb0\xc1<\xbb\xad7J\\\xca\xf1K6\x0e$\xab\xbf\xc3\x84w\xfd\xa8P\xf4\xb1\xe7\xc6/\xd2-Y\x8dz\nN\x96iR\x02\xaa6\x98\xec\xfc\xfa\x14\x83\x0c7\x1f\xc9\x927\x7f\xd7\x81-\xe5\xc4\xb9\xe0>4\xfeg\xf4>f\xd1\xd3\xf4@\xcf^b4\x0fp\x06> \x18Z\xce\xf7r\x1b%l\x93t\x14\x93\xa3s\xb1\x0e\xaa\x87\x10#Y_ \x05\x19G\xedh\x8e\x92\x12\xca]\xfc\xd6+\xb2\x92< \xcf\xe1_\xaf \xbe\xa0v\x7fv\xe5\xec\x1a\x86a3>\xe7{\xb4?\xa0\x0f\x8e\xa2\xb9jo\xb9\x87&oAl\x88\xa6P\xfdjc\x0b\xf5\x93\x16\x1b\x19\x8f\xb2\xed\x9b\x9b\xc9\x90\xf5\x01\x0c\xdf\x19=\x0f>8#\xc9/\x8f\x0c\xd1\x02\xfe\x1bU\xd6N\xd0\x8d<\xafO\xa4\xf3\xfe\xaf*\xd3\x05\x9dy\x08\xa1\xcbW1\xb4\x9c\xc8\x90\xb2g\xf4\x18\x16\x93:\xfcG\xd8\xd1x\x961\x1f\xba+\x0c_]\x99\xadc\x89Z$ v\xd8\xdf\xfd\xabN\xa6\"\xaa\x9d^\xe6\'\x8a}h)\xa3\xe7\x8a\xb8\xda\x11\xbb\x17-\x99\x9d\x13$F\xf7\xc5\xe2\xd8\x9f\x8e\x7f\xc7\x8ftmZ\xb2\xe8r\xf5\xac\xee$\x10\xad/\x14\xda\xff-\x9aFqG\xbeB\xdf\xbb\x01\xdb\xf4\x7f\xd3(\x8f1Y[\xd3\xc9\x02\xa6\xb4R\xcan\x97\xfbC\xc5\x08&o\x8a\xf4\xbb\xfd\x9f(\xaa\r\xd5E\xf3\x13:\x1d\xd8\xc0x\x8fAg<\x1e\x94d\xae{\x0b\xc5\xe8\xd9\x01\x889\x1a\x97\x86dA\xd5;\x87\x0cn\xfa\x0f\xc6\xbdH\x14\xbf9M\xd4\x9eA\xb6\x8f\x96\x1dc\x96\x93\xd9\x95\x06x1h\x9e7\x06;\x80\x89Ea9#\xc7\x1bD\xa3\x15\xe5\x1c\xf8\x920\xbb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DST Root CA X3 O=Digital Signature Trust Co.
     * Subject: CN=DST Root CA X3 O=Digital Signature Trust Co.
     * Label: "DST Root CA X3"
     * Serial: 91299735575339953335919266965803778155
     * MD5 Fingerprint: 41:03:52:dc:0f:f7:50:1b:16:f0:02:8e:ba:6f:45:c5
     * SHA1 Fingerprint: da:c9:02:4f:54:d8:f6:df:94:93:5f:b1:73:26:38:ca:6a:d7:7c:13
     * SHA256 Fingerprint: 06:87:26:03:31:a7:24:03:d9:09:f1:05:e6:9b:cf:0d:32:e1:bd:24:93:ff:c6:d9:20:6d:11:bc:d6:77:07:39
     * -----BEGIN CERTIFICATE-----
     * MIIDSjCCAjKgAwIBAgIQRK+wgNajJ7qJMDmGLvhAazANBgkqhkiG9w0BAQUFADA/
     * MSQwIgYDVQQKExtEaWdpdGFsIFNpZ25hdHVyZSBUcnVzdCBDby4xFzAVBgNVBAMT
     * DkRTVCBSb290IENBIFgzMB4XDTAwMDkzMDIxMTIxOVoXDTIxMDkzMDE0MDExNVow
     * PzEkMCIGA1UEChMbRGlnaXRhbCBTaWduYXR1cmUgVHJ1c3QgQ28uMRcwFQYDVQQD
     * Ew5EU1QgUm9vdCBDQSBYMzCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEB
     * AN+v6ZdQCINXtMxiZfaQguzH0yxrMMpb7NnDfcdAwRgUi+DoM3ZJKuM/IUmTrE4O
     * rz5Iy2Xu/NMhD2XSKtkyj4zl93ewEnu1lcCJo6m67XMuegwGMoOifooUMM0RoOEq
     * OLl5CjH9UL2AZd+3UWODyOKIYepLYYHsUmu5ouJLGiifSKOeDNoJjj4XLh7dIN9b
     * xiqKqy69cK3FCxolkHRyxXtqqzTWMIn/5WgTe1QLyNau7Fqckh49ZLOMxt+/yUFw
     * 7BZy1SbsOFU5Q9D8/RhcQPGX69Wam40dutolucbY38EVAjqr2m7xPi71XAicPNaD
     * aeQQmxkqtilX4+U9m5/wAl0CAwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNV
     * HQ8BAf8EBAMCAQYwHQYDVR0OBBYEFMSnsaR7LHH62+FLkHX/xBVghYkQMA0GCSqG
     * SIb3DQEBBQUAA4IBAQCjGiybFwBcqR7uKGY3Or+Dxz9LwwmglSBd49lZRNI+DT69
     * ikugdB/OEIKcdBodfpga3csTS7MgROSR6cz8faXbauX+5v3gTt23ADq1cEmv8uXr
     * AvHRAosZy5Q6XkjEGB5YGV8eAlrwDPGxrancWYaLbumR9YbK+rlmM6pZW87ipxZz
     * R8srzJmwN0jP41ZL9c8PDHIyh8bwRLtTcm1D9SZImlJnt1ir/md2cXjbDaJWFBM5
     * JDGFoqgCWjBH4d1QB7wCCZAA62RjYJsWvIjJEubSfZGL+T0yjWW06XyxV3bqxbYo
     * Ob8VZRzI9neWagqNdwvYkQsEjgfbKbYK7p2CNTUQ
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1$0\"\x06\x03U\x04\n\x13\x1bDigital Signature Trust Co.1\x170\x15\x06\x03U\x04\x03\x13\x0eDST Root CA X3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xdf\xaf\xe9\x97P\x08\x83W\xb4\xccbe\xf6\x90\x82\xec\xc7\xd3,k0\xca[\xec\xd9\xc3}\xc7@\xc1\x18\x14\x8b\xe0\xe83vI*\xe3?!I\x93\xacN\x0e\xaf>H\xcbe\xee\xfc\xd3!\x0fe\xd2*\xd92\x8f\x8c\xe5\xf7w\xb0\x12{\xb5\x95\xc0\x89\xa3\xa9\xba\xeds.z\x0c\x062\x83\xa2~\x8a\x140\xcd\x11\xa0\xe1*8\xb9y\n1\xfdP\xbd\x80e\xdf\xb7Qc\x83\xc8\xe2\x88a\xeaKa\x81\xecRk\xb9\xa2\xe2K\x1a(\x9fH\xa3\x9e\x0c\xda\t\x8e>\x17.\x1e\xdd \xdf[\xc6*\x8a\xab.\xbdp\xad\xc5\x0b\x1a%\x90tr\xc5{j\xab4\xd60\x89\xff\xe5h\x13{T\x0b\xc8\xd6\xae\xecZ\x9c\x92\x1e=d\xb3\x8c\xc6\xdf\xbf\xc9Ap\xec\x16r\xd5&\xec8U9C\xd0\xfc\xfd\x18\\@\xf1\x97\xeb\xd5\x9a\x9b\x8d\x1d\xba\xda%\xb9\xc6\xd8\xdf\xc1\x15\x02:\xab\xdan\xf1>.\xf5\\\x08\x9c<\xd6\x83i\xe4\x10\x9b\x19*\xb6)W\xe3\xe5=\x9b\x9f\xf0\x02]\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TrustCor RootCert CA-2 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Subject: CN=TrustCor RootCert CA-2 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Label: "TrustCor RootCert CA-2"
     * Serial: 2711694510199101698
     * MD5 Fingerprint: a2:e1:f8:18:0b:ba:45:d5:c7:41:2a:bb:37:52:45:64
     * SHA1 Fingerprint: b8:be:6d:cb:56:f1:55:b9:63:d4:12:ca:4e:06:34:c7:94:b2:1c:c0
     * SHA256 Fingerprint: 07:53:e9:40:37:8c:1b:d5:e3:83:6e:39:5d:ae:a5:cb:83:9e:50:46:f1:bd:0e:ae:19:51:cf:10:fe:c7:c9:65
     * -----BEGIN CERTIFICATE-----
     * MIIGLzCCBBegAwIBAgIIJaHfyjPLWQIwDQYJKoZIhvcNAQELBQAwgaQxCzAJBgNV
     * BAYTAlBBMQ8wDQYDVQQIDAZQYW5hbWExFDASBgNVBAcMC1BhbmFtYSBDaXR5MSQw
     * IgYDVQQKDBtUcnVzdENvciBTeXN0ZW1zIFMuIGRlIFIuTC4xJzAlBgNVBAsMHlRy
     * dXN0Q29yIENlcnRpZmljYXRlIEF1dGhvcml0eTEfMB0GA1UEAwwWVHJ1c3RDb3Ig
     * Um9vdENlcnQgQ0EtMjAeFw0xNjAyMDQxMjMyMjNaFw0zNDEyMzExNzI2MzlaMIGk
     * MQswCQYDVQQGEwJQQTEPMA0GA1UECAwGUGFuYW1hMRQwEgYDVQQHDAtQYW5hbWEg
     * Q2l0eTEkMCIGA1UECgwbVHJ1c3RDb3IgU3lzdGVtcyBTLiBkZSBSLkwuMScwJQYD
     * VQQLDB5UcnVzdENvciBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkxHzAdBgNVBAMMFlRy
     * dXN0Q29yIFJvb3RDZXJ0IENBLTIwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIK
     * AoICAQCnIG7CKqJiJJWQdsg4foDSq8GbZQWU9MEKENUCrO2fk8eHyLAnK0IMPQo+
     * QVqedd2NyuCb7GgypGmSaIwLgQ5WoD4a3SwlFIIvl9NkRvRUqdw6VC0xK5mC8tkq
     * 1+9xALgxpL56JAfDQiDyitSSBBtlVkxs1Pu2YVpHI7TYabS3OtB0PAx1oYxOdqHp
     * 2yqlO/rOsP9+aij9JxzIsekp8VduZLTQwRVtDr4uDkbIXvRR/u8OYzo7cbrPb1nK
     * DOObXUm4TOJXsZiKQlecdu/vvdFoqNL0Cbt3Nb4lggjEFixEIFapRBF37120Hape
     * az6LMvYHL1cEksr1/p3C6eizjkxLAjHZ5DxIgif3GIJ2SDpxsROhOdUuxTTCHWKF
     * 3wP+TfSvPd9cW436cOGlfifHhi5qjxLGhF5DUVCcGZt45vz27Ud+ez1m7xMTiF88
     * oWP7+ayHNZ/zgp6kPwqcMWmLmaSISo5uZk3vFsQPeSghYA2FFn3XVDjxklb9tTNM
     * g9zXEJ9L/cb4Qr26fHMC4P99zVvh1Kxhe1fVSntb1IVYJ12/+CtgrKAmrhQhJ8Z3
     * mjOAPF5GP/fDsaOGM8boXg25NSyqRsGFAnWAoOsk+xWq5Gd/bnc/9ASKL3x74xdh
     * 8N0JqSDIvgmk0H5Ew7IwSjiqqewYmgeCK9u4nBit2uBGF6zPXQIDAQABo2MwYTAd
     * BgNVHQ4EFgQU2f4hQG6UnrybPZx9mCAZ5YwwYrIwHwYDVR0jBBgwFoAU2f4hQG6U
     * nrybPZx9mCAZ5YwwYrIwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAYYw
     * DQYJKoZIhvcNAQELBQADggIBAJ5Fngw7tu/hOsh80QA9z+LqBrWyOrsGS2h60COX
     * dKcs8AjYeVrXWoSK2BKaG9l9XE1wxaX5q+WjiYndAfrs3fnpkpfbsEZC89NiqpX+
     * MWcUaViQCqoL7jcjx1BRtPV+nuN79+TMQjItSQzL/0kMmx40/W5ulop5A7Zv2wnL
     * /V9lFDfhOPXzYRZY5LVtDQsEGz9QLX+zx3oaFoBg+Iof6Rsqxvm6ARppv9JYx1RX
     * CI/hOWB3S6xZhBqI8d3LT3jX5+EzLfzuQfogsL7L9ziUwOHQhQ+77Sxzq+3+knYa
     * ZH9bDTMJBzN7Bj8RpFxwPIXAz+OQqIN3+tvmxYxoZxBnpVIt8MSZj3+/0WvitUfW
     * 2dCFmU2Umw9Lje4AWkcdEQOsQRivh7dvDDqPys/cA8GiCcjl/YBeyGBCARsaU1q7
     * N6a3vLqE6R5sGtRk2tRD/pOLS/IseRYQ1JMLiI+h2IYURpFHmygk71dSTlxCnKr3
     * Sewn6EAes6aJInKc9Q0ztFijMDvd1GpUk74aTfOTlPf8hAs/hCBcNANExdqtvArB
     * As8e5ZTZ845b2EzwnexhF7sUMlQMAimTHpKG9n/v55IFDlndmQguLvqcAFLTxWYp
     * 5KeXRKQOKIETNcX2b2TmQcTVL8w0RSXPQQCWPUouwpaYT05KnJe32x+SMsj/D1Fu
     * 1uwJ
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PA1\x0f0\r\x06\x03U\x04\x08\x0c\x06Panama1\x140\x12\x06\x03U\x04\x07\x0c\x0bPanama City1$0\"\x06\x03U\x04\n\x0c\x1bTrustCor Systems S. de R.L.1\'0%\x06\x03U\x04\x0b\x0c\x1eTrustCor Certificate Authority1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16TrustCor RootCert CA-2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa7 n\xc2*\xa2b$\x95\x90v\xc88~\x80\xd2\xab\xc1\x9be\x05\x94\xf4\xc1\n\x10\xd5\x02\xac\xed\x9f\x93\xc7\x87\xc8\xb0\'+B\x0c=\n>AZ\x9eu\xdd\x8d\xca\xe0\x9b\xech2\xa4i\x92h\x8c\x0b\x81\x0eV\xa0>\x1a\xdd,%\x14\x82/\x97\xd3dF\xf4T\xa9\xdc:T-1+\x99\x82\xf2\xd9*\xd7\xefq\x00\xb81\xa4\xbez$\x07\xc3B \xf2\x8a\xd4\x92\x04\x1beVLl\xd4\xfb\xb6aZG#\xb4\xd8i\xb4\xb7:\xd0t<\x0cu\xa1\x8cNv\xa1\xe9\xdb*\xa5;\xfa\xce\xb0\xff~j(\xfd\'\x1c\xc8\xb1\xe9)\xf1Wnd\xb4\xd0\xc1\x15m\x0e\xbe.\x0eF\xc8^\xf4Q\xfe\xef\x0ec:;q\xba\xcfoY\xca\x0c\xe3\x9b]I\xb8L\xe2W\xb1\x98\x8aBW\x9cv\xef\xef\xbd\xd1h\xa8\xd2\xf4\t\xbbw5\xbe%\x82\x08\xc4\x16,D V\xa9D\x11w\xef]\xb4\x1d\xaa^k>\x8b2\xf6\x07/W\x04\x92\xca\xf5\xfe\x9d\xc2\xe9\xe8\xb3\x8eLK\x021\xd9\xe4<H\x82\'\xf7\x18\x82vH:q\xb1\x13\xa19\xd5.\xc54\xc2\x1db\x85\xdf\x03\xfeM\xf4\xaf=\xdf\\[\x8d\xfap\xe1\xa5~\'\xc7\x86.j\x8f\x12\xc6\x84^CQP\x9c\x19\x9bx\xe6\xfc\xf6\xedG~{=f\xef\x13\x13\x88_<\xa1c\xfb\xf9\xac\x875\x9f\xf3\x82\x9e\xa4?\n\x9c1i\x8b\x99\xa4\x88J\x8enfM\xef\x16\xc4\x0fy(!`\r\x85\x16}\xd7T8\xf1\x92V\xfd\xb53L\x83\xdc\xd7\x10\x9fK\xfd\xc6\xf8B\xbd\xba|s\x02\xe0\xff}\xcd[\xe1\xd4\xaca{W\xd5J{[\xd4\x85X\']\xbf\xf8+`\xac\xa0&\xae\x14!\'\xc6w\x9a3\x80<^F?\xf7\xc3\xb1\xa3\x863\xc6\xe8^\r\xb95,\xaaF\xc1\x85\x02u\x80\xa0\xeb$\xfb\x15\xaa\xe4g\x7fnw?\xf4\x04\x8a/|{\xe3\x17a\xf0\xdd\t\xa9 \xc8\xbe\t\xa4\xd0~D\xc3\xb20J8\xaa\xa9\xec\x18\x9a\x07\x82+\xdb\xb8\x9c\x18\xad\xda\xe0F\x17\xac\xcf]\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=AffirmTrust Networking O=AffirmTrust
     * Subject: CN=AffirmTrust Networking O=AffirmTrust
     * Label: "AffirmTrust Networking"
     * Serial: 8957382827206547757
     * MD5 Fingerprint: 42:65:ca:be:01:9a:9a:4c:a9:8c:41:49:cd:c0:d5:7f
     * SHA1 Fingerprint: 29:36:21:02:8b:20:ed:02:f5:66:c5:32:d1:d6:ed:90:9f:45:00:2f
     * SHA256 Fingerprint: 0a:81:ec:5a:92:97:77:f1:45:90:4a:f3:8d:5d:50:9f:66:b5:e2:c5:8f:cd:b5:31:05:8b:0e:17:f3:f0:b4:1b
     * -----BEGIN CERTIFICATE-----
     * MIIDTDCCAjSgAwIBAgIIfE8EORzUmS0wDQYJKoZIhvcNAQEFBQAwRDELMAkGA1UE
     * BhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MR8wHQYDVQQDDBZBZmZpcm1UcnVz
     * dCBOZXR3b3JraW5nMB4XDTEwMDEyOTE0MDgyNFoXDTMwMTIzMTE0MDgyNFowRDEL
     * MAkGA1UEBhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MR8wHQYDVQQDDBZBZmZp
     * cm1UcnVzdCBOZXR3b3JraW5nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC
     * AQEAtITMMxcua5Rsa2FSoOujz3mUTOWUgJnLVWREZY9nZOIG41w3SfYvm4SEHi3y
     * YJ0wTsyEheIszx6e/jarM3c1RNg1lho9Nuh6DtjVR6FqaYvZ/Ls6rnla1fTWcbua
     * kCNrmreIdIcMHl+5ni36q1Mr3Lt2PpNMCAiMHqIjHNRqrSK6mQEubWXLviRmVSRL
     * QESxG9fhwoXA3hA/Pe24/PHxI1Pcv2WXb9n5QHGNfb2V1M6+oF4nI979ptAmDgAp
     * 6zxG8D1gvz9Q0twmQVGeFDdCBKNwV6gbh+0t+nvujArjqWaJGctB+d1ENmHP4ndG
     * yH329JKBNv3bNPFyfvMMFr20FQIDAQABo0IwQDAdBgNVHQ4EFgQUBx/S55zawm6i
     * QLSwelAQUHTEyL0wDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwDQYJ
     * KoZIhvcNAQEFBQADggEBAIlXshZ6qML91tmbmzTCnLQyFE2npN/svqe++EPbkTfO
     * tDIuUFUaNU52Q3Eg75N3ThVwLofDwR1t3Mu1J9QsVtFSUzpE0nPIxBsFZVpikpzu
     * QY0x2+c06lkh1QF612S4ZDnNye2v7UsDSKegmQGA3GWjNq5lWUhPgkvIZfFXHeVZ
     * Lgo/bNjR9eUJtGxUAArgFU2HdW23WJZa3W3SAKD0m0i+wzekujbgfIeFlxoVot4u
     * olu9rxj5kFDNcFn4J2dHy8egBzp90SxdbBk6ZrV9/ZFvgrG+CJPbFEfxojfHRZ48
     * x3evZKiT3/Zpg4Jg8klCNO1aAFSFHBY2kgxc+qatv9s=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x140\x12\x06\x03U\x04\n\x0c\x0bAffirmTrust1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16AffirmTrust Networking",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb4\x84\xcc3\x17.k\x94lkaR\xa0\xeb\xa3\xcfy\x94L\xe5\x94\x80\x99\xcbUdDe\x8fgd\xe2\x06\xe3\\7I\xf6/\x9b\x84\x84\x1e-\xf2`\x9d0N\xcc\x84\x85\xe2,\xcf\x1e\x9e\xfe6\xab3w5D\xd85\x96\x1a=6\xe8z\x0e\xd8\xd5G\xa1ji\x8b\xd9\xfc\xbb:\xaeyZ\xd5\xf4\xd6q\xbb\x9a\x90#k\x9a\xb7\x88t\x87\x0c\x1e_\xb9\x9e-\xfa\xabS+\xdc\xbbv>\x93L\x08\x08\x8c\x1e\xa2#\x1c\xd4j\xad\"\xba\x99\x01.me\xcb\xbe$fU$K@D\xb1\x1b\xd7\xe1\xc2\x85\xc0\xde\x10?=\xed\xb8\xfc\xf1\xf1#S\xdc\xbfe\x97o\xd9\xf9@q\x8d}\xbd\x95\xd4\xce\xbe\xa0^\'#\xde\xfd\xa6\xd0&\x0e\x00)\xeb<F\xf0=`\xbf?P\xd2\xdc&AQ\x9e\x147B\x04\xa3pW\xa8\x1b\x87\xed-\xfa{\xee\x8c\n\xe3\xa9f\x89\x19\xcbA\xf9\xddD6a\xcf\xe2wF\xc8}\xf6\xf4\x92\x816\xfd\xdb4\xf1r~\xf3\x0c\x16\xbd\xb4\x15\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=COMODO Certification Authority O=COMODO CA Limited
     * Subject: CN=COMODO Certification Authority O=COMODO CA Limited
     * Label: "COMODO Certification Authority"
     * Serial: 104350513648249232941998508985834464573
     * MD5 Fingerprint: 5c:48:dc:f7:42:72:ec:56:94:6d:1c:cc:71:35:80:75
     * SHA1 Fingerprint: 66:31:bf:9e:f7:4f:9e:b6:c9:d5:a6:0c:ba:6a:be:d1:f7:bd:ef:7b
     * SHA256 Fingerprint: 0c:2c:d6:3d:f7:80:6f:a3:99:ed:e8:09:11:6b:57:5b:f8:79:89:f0:65:18:f9:80:8c:86:05:03:17:8b:af:66
     * -----BEGIN CERTIFICATE-----
     * MIIEHTCCAwWgAwIBAgIQToEtioJl4AsC7j41AkblPTANBgkqhkiG9w0BAQUFADCB
     * gTELMAkGA1UEBhMCR0IxGzAZBgNVBAgTEkdyZWF0ZXIgTWFuY2hlc3RlcjEQMA4G
     * A1UEBxMHU2FsZm9yZDEaMBgGA1UEChMRQ09NT0RPIENBIExpbWl0ZWQxJzAlBgNV
     * BAMTHkNPTU9ETyBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTAeFw0wNjEyMDEwMDAw
     * MDBaFw0yOTEyMzEyMzU5NTlaMIGBMQswCQYDVQQGEwJHQjEbMBkGA1UECBMSR3Jl
     * YXRlciBNYW5jaGVzdGVyMRAwDgYDVQQHEwdTYWxmb3JkMRowGAYDVQQKExFDT01P
     * RE8gQ0EgTGltaXRlZDEnMCUGA1UEAxMeQ09NT0RPIENlcnRpZmljYXRpb24gQXV0
     * aG9yaXR5MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA0ECLi3LjkRv3
     * UcEbVASY06m/weaKXTuH+7uIzg3jLz8GlvCiKVCZrts7oVewdFFxze1CkU1B/qnI
     * 2GqGd0S7WWaXUF601CxwRM/aN5VCaTwwxHGzUvAhTaHYujl8HJ6jJJ3ygxaYqhZ8
     * Q5sVW7euNJH+1GImGEaaP+vB+fGQV+useg2L23IwambV4EajcNxo2f8ESIl33rXp
     * +2dtQem8Ob0y2WIC8bGoPW43nOIv4tOiJovGuFVDiOEjPqXSJDlqR6sA1KGzqSX+
     * DT+nHbrTUcELpNqsOO9VUCQFZUaTNE8tja3G1CEZ0o7KBWFxB3NH5YoZEr0ETc5O
     * nKVIrLsm9wIDAQABo4GOMIGLMB0GA1UdDgQWBBQLWOWLxkwVN6RAqTCpIb5HNlpW
     * /zAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zBJBgNVHR8EQjBAMD6g
     * PKA6hjhodHRwOi8vY3JsLmNvbW9kb2NhLmNvbS9DT01PRE9DZXJ0aWZpY2F0aW9u
     * QXV0aG9yaXR5LmNybDANBgkqhkiG9w0BAQUFAAOCAQEAPpiem/Yb6dc5t3iuHXIY
     * SdOH5EOC6z/JqvWote9VfCFSZfnVDeFs9D6Mk3ORLgLETgdxb8CPOGEIqB6BCsAv
     * IC9Bi5HcSEW88cbeunZrM8gALTFGTO3nnc+IlP8zwFboJIYmuNg4ON8qa90SzMc/
     * RxdMosIGlgnW2/4/PEZB31jiVg88O8EckzXZOFKs7sjsLjBOlDW0JB9LeGna8gI4
     * zJVSk/BwJVmcIGfE7vmLV2H0knZ9P4SNVbfo5azV8fUZVqZa+5Acr5Pr5RzUZ5dd
     * BA6+C4OmF4O5MBKgxTMVBbkN+8cFduPYSo38NBejxiEovjBFMR7HeL5YYTisO+IB
     * ZQ==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GB1\x1b0\x19\x06\x03U\x04\x08\x13\x12Greater Manchester1\x100\x0e\x06\x03U\x04\x07\x13\x07Salford1\x1a0\x18\x06\x03U\x04\n\x13\x11COMODO CA Limited1\'0%\x06\x03U\x04\x03\x13\x1eCOMODO Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd0@\x8b\x8br\xe3\x91\x1b\xf7Q\xc1\x1bT\x04\x98\xd3\xa9\xbf\xc1\xe6\x8a];\x87\xfb\xbb\x88\xce\r\xe3/?\x06\x96\xf0\xa2)P\x99\xae\xdb;\xa1W\xb0tQq\xcd\xedB\x91MA\xfe\xa9\xc8\xd8j\x86wD\xbbYf\x97P^\xb4\xd4,pD\xcf\xda7\x95Bi<0\xc4q\xb3R\xf0!M\xa1\xd8\xba9|\x1c\x9e\xa3$\x9d\xf2\x83\x16\x98\xaa\x16|C\x9b\x15[\xb7\xae4\x91\xfe\xd4b&\x18F\x9a?\xeb\xc1\xf9\xf1\x90W\xeb\xacz\r\x8b\xdbr0jf\xd5\xe0F\xa3p\xdch\xd9\xff\x04H\x89w\xde\xb5\xe9\xfbgmA\xe9\xbc9\xbd2\xd9b\x02\xf1\xb1\xa8=n7\x9c\xe2/\xe2\xd3\xa2&\x8b\xc6\xb8UC\x88\xe1#>\xa5\xd2$9jG\xab\x00\xd4\xa1\xb3\xa9%\xfe\r?\xa7\x1d\xba\xd3Q\xc1\x0b\xa4\xda\xac8\xefUP$\x05eF\x934O-\x8d\xad\xc6\xd4!\x19\xd2\x8e\xca\x05aq\x07sG\xe5\x8a\x19\x12\xbd\x04M\xceN\x9c\xa5H\xac\xbb&\xf7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=emSign Root CA - C1 O=eMudhra Inc OU=emSign PKI
     * Subject: CN=emSign Root CA - C1 O=eMudhra Inc OU=emSign PKI
     * Label: "emSign Root CA - C1"
     * Serial: 825510296613316004955058
     * MD5 Fingerprint: d8:e3:5d:01:21:fa:78:5a:b0:df:ba:d2:ee:2a:5f:68
     * SHA1 Fingerprint: e7:2e:f1:df:fc:b2:09:28:cf:5d:d4:d5:67:37:b1:51:cb:86:4f:01
     * SHA256 Fingerprint: 12:56:09:aa:30:1d:a0:a2:49:b9:7a:82:39:cb:6a:34:21:6f:44:dc:ac:9f:39:54:b1:42:92:f2:e8:c8:60:8f
     * -----BEGIN CERTIFICATE-----
     * MIIDczCCAlugAwIBAgILAK7PALrEzzL4Q7IwDQYJKoZIhvcNAQELBQAwVjELMAkG
     * A1UEBhMCVVMxEzARBgNVBAsTCmVtU2lnbiBQS0kxFDASBgNVBAoTC2VNdWRocmEg
     * SW5jMRwwGgYDVQQDExNlbVNpZ24gUm9vdCBDQSAtIEMxMB4XDTE4MDIxODE4MzAw
     * MFoXDTQzMDIxODE4MzAwMFowVjELMAkGA1UEBhMCVVMxEzARBgNVBAsTCmVtU2ln
     * biBQS0kxFDASBgNVBAoTC2VNdWRocmEgSW5jMRwwGgYDVQQDExNlbVNpZ24gUm9v
     * dCBDQSAtIEMxMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAz+upufGZ
     * BczYKCFK83M0UYRWEPWgTywS4/oTmifQz/l5GnRfHXk5/Fv4cI7gklL35CX5VIPZ
     * HdPIWoU/Xse2B+4+wM6ar6xWQio5JXDWv7V7Nq2s9nPczdcdioOl+yuQFTdrHCZH
     * 3DspVpNqs8FqOp099cGXOFgFixwR4+S0uF2FHYP+eF8LRWgYSKVGczQ7/g/IdrvH
     * GPMF0Ybzhe3nudkyrVWIzqa2kbBPrH4VI5b2P/AgNBbeCsbEBEV5f6f9vtKppa+c
     * xSMq9zwhbL2vj07FOrLzNBL834AaSaTUqZX3noleoomslMuoaJuvimUnzYnu3Yy1
     * aylwQ6BpC+S5DwIDAQABo0IwQDAdBgNVHQ4EFgQU/qHgcB4qAzlSWkK+XJGFehiq
     * TbUwDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQEL
     * BQADggEBAMJKVvoVIXsoounlHfv4LcQ5lkFMOycsxGwYFYDGrK9HWS8mC+M2sO87
     * /kOXSTKZEhVb3xEp/6tT+LvBeA+snFOvV71ojD1pM/CjoCNjO2RnIkSt1XHLVip4
     * kqNPEjE2NuLe/gDEo2APJ62gsIq1NnpSob0n9CAnYuhNlCQT5AoE6TyrLshDCUrG
     * YQTlSTR+08TI9Q/Aqum6VF7zYytPT1DU/rl7mYw9wC68AivTxEDkigcxHpvOJpkT
     * +xHqmiIMERnHXhuBUDDIlhJu58tBf5E7oke3VIAb3ADMmpDqw8NQBmIMMMAVSKeo
     * WXzhriKi4gp6D/piq1JM4fHfyr6DDUI=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x130\x11\x06\x03U\x04\x0b\x13\nemSign PKI1\x140\x12\x06\x03U\x04\n\x13\x0beMudhra Inc1\x1c0\x1a\x06\x03U\x04\x03\x13\x13emSign Root CA - C1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xcf\xeb\xa9\xb9\xf1\x99\x05\xcc\xd8(!J\xf3s4Q\x84V\x10\xf5\xa0O,\x12\xe3\xfa\x13\x9a\'\xd0\xcf\xf9y\x1at_\x1dy9\xfc[\xf8p\x8e\xe0\x92R\xf7\xe4%\xf9T\x83\xd9\x1d\xd3\xc8Z\x85?^\xc7\xb6\x07\xee>\xc0\xce\x9a\xaf\xacVB*9%p\xd6\xbf\xb5{6\xad\xac\xf6s\xdc\xcd\xd7\x1d\x8a\x83\xa5\xfb+\x90\x157k\x1c&G\xdc;)V\x93j\xb3\xc1j:\x9d=\xf5\xc1\x978X\x05\x8b\x1c\x11\xe3\xe4\xb4\xb8]\x85\x1d\x83\xfex_\x0bEh\x18H\xa5Fs4;\xfe\x0f\xc8v\xbb\xc7\x18\xf3\x05\xd1\x86\xf3\x85\xed\xe7\xb9\xd92\xadU\x88\xce\xa6\xb6\x91\xb0O\xac~\x15#\x96\xf6?\xf0 4\x16\xde\n\xc6\xc4\x04Ey\x7f\xa7\xfd\xbe\xd2\xa9\xa5\xaf\x9c\xc5#*\xf7<!l\xbd\xaf\x8fN\xc5:\xb2\xf34\x12\xfc\xdf\x80\x1aI\xa4\xd4\xa9\x95\xf7\x9e\x89^\xa2\x89\xac\x94\xcb\xa8h\x9b\xaf\x8ae\'\xcd\x89\xee\xdd\x8c\xb5k)pC\xa0i\x0b\xe4\xb9\x0f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=Starfield Technologies, Inc. OU=Starfield Class 2 Certification Authority
     * Subject: O=Starfield Technologies, Inc. OU=Starfield Class 2 Certification Authority
     * Label: "Starfield Class 2 CA"
     * Serial: 0
     * MD5 Fingerprint: 32:4a:4b:bb:c8:63:69:9b:be:74:9a:c6:dd:1d:46:24
     * SHA1 Fingerprint: ad:7e:1c:28:b0:64:ef:8f:60:03:40:20:14:c3:d0:e3:37:0e:b5:8a
     * SHA256 Fingerprint: 14:65:fa:20:53:97:b8:76:fa:a6:f0:a9:95:8e:55:90:e4:0f:cc:7f:aa:4f:b7:c2:c8:67:75:21:fb:5f:b6:58
     * -----BEGIN CERTIFICATE-----
     * MIIEDzCCAvegAwIBAgIBADANBgkqhkiG9w0BAQUFADBoMQswCQYDVQQGEwJVUzEl
     * MCMGA1UEChMcU3RhcmZpZWxkIFRlY2hub2xvZ2llcywgSW5jLjEyMDAGA1UECxMp
     * U3RhcmZpZWxkIENsYXNzIDIgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMDQw
     * NjI5MTczOTE2WhcNMzQwNjI5MTczOTE2WjBoMQswCQYDVQQGEwJVUzElMCMGA1UE
     * ChMcU3RhcmZpZWxkIFRlY2hub2xvZ2llcywgSW5jLjEyMDAGA1UECxMpU3RhcmZp
     * ZWxkIENsYXNzIDIgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwggEgMA0GCSqGSIb3
     * DQEBAQUAA4IBDQAwggEIAoIBAQC3Msj+6XGmBIWtDBFk385N78gDGIc/oav7PKaf
     * 8MOh2tTYbitTkPskpD6E8J7oX+zlJ0T1KKY/e97gKvDIr1MvnsoFAZMej2YcOadN
     * +lq2cwQlZut3f+dZxkqZJRRU6ybH838Z1TBwj6+wRir/resp7defqgSHo9T5iaU0
     * X9tDkYI22WY8sbi5gv2cOj4QyDvvBmVmepsZGD3/cVE8MC5fvj13c7JdBmzDI1aa
     * K4UmkhynArPkPw2vCHmCuDY96pzTNbO8acr1zJ3o/WSNF4Azbl5KXZnJHoe0nRrA
     * 1W4TNSNe35tfPe/W93bC6j67eA0cQmdrBNj41tpvi/JEoAGrAgEDo4HFMIHCMB0G
     * A1UdDgQWBBS/X7fRzt0fhvRbVazc1xDCDqmI5zCBkgYDVR0jBIGKMIGHgBS/X7fR
     * zt0fhvRbVazc1xDCDqmI56FspGowaDELMAkGA1UEBhMCVVMxJTAjBgNVBAoTHFN0
     * YXJmaWVsZCBUZWNobm9sb2dpZXMsIEluYy4xMjAwBgNVBAsTKVN0YXJmaWVsZCBD
     * bGFzcyAyIENlcnRpZmljYXRpb24gQXV0aG9yaXR5ggEAMAwGA1UdEwQFMAMBAf8w
     * DQYJKoZIhvcNAQEFBQADggEBAAWdP4id0ckaVaGsafPzWdqbAYcaT1epoXkJKtv3
     * L7IezMdeatiDh6GX70k1PncGQVhiv45YuApnP+yz3SFmH8lU+nLMPUxA2IGvd56D
     * eruix/U0F47ZEUD0/CwqTRV/p2JdLiXTAAsgGh1o+Re49L2L7ShZ3U0WixeDyLJl
     * xy16paq8U4Zt3VekyvggQQto8PT7dL5WXXp59fkdheMtlb71cZBDzI0fmgAKhynp
     * VSJYACPq4xJDKVtHCN2MQWplBqjlIapBtJUhlbl90TSrE9atvNziPTnNvT51cKEY
     * WQPJIrSPnNVeKtelttQKbfi3QBFGmh95DmK/D5fs4C8fF5Q=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1%0#\x06\x03U\x04\n\x13\x1cStarfield Technologies, Inc.1200\x06\x03U\x04\x0b\x13)Starfield Class 2 Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\r\x000\x82\x01\x08\x02\x82\x01\x01\x00\xb72\xc8\xfe\xe9q\xa6\x04\x85\xad\x0c\x11d\xdf\xceM\xef\xc8\x03\x18\x87?\xa1\xab\xfb<\xa6\x9f\xf0\xc3\xa1\xda\xd4\xd8n+S\x90\xfb$\xa4>\x84\xf0\x9e\xe8_\xec\xe5\'D\xf5(\xa6?{\xde\xe0*\xf0\xc8\xafS/\x9e\xca\x05\x01\x93\x1e\x8ff\x1c9\xa7M\xfaZ\xb6s\x04%f\xebw\x7f\xe7Y\xc6J\x99%\x14T\xeb&\xc7\xf3\x7f\x19\xd50p\x8f\xaf\xb0F*\xff\xad\xeb)\xed\xd7\x9f\xaa\x04\x87\xa3\xd4\xf9\x89\xa54_\xdbC\x91\x826\xd9f<\xb1\xb8\xb9\x82\xfd\x9c:>\x10\xc8;\xef\x06efz\x9b\x19\x18=\xffqQ<0._\xbe=ws\xb2]\x06l\xc3#V\x9a+\x85&\x92\x1c\xa7\x02\xb3\xe4?\r\xaf\x08y\x82\xb86=\xea\x9c\xd35\xb3\xbci\xca\xf5\xcc\x9d\xe8\xfdd\x8d\x17\x803n^J]\x99\xc9\x1e\x87\xb4\x9d\x1a\xc0\xd5n\x135#^\xdf\x9b_=\xef\xd6\xf7v\xc2\xea>\xbbx\r\x1cBgk\x04\xd8\xf8\xd6\xdao\x8b\xf2D\xa0\x01\xab\x02\x01\x03",
        name_constraints: None
    },

    /*
     * Issuer: CN=GTS Root R3 O=Google Trust Services LLC
     * Subject: CN=GTS Root R3 O=Google Trust Services LLC
     * Label: "GTS Root R3"
     * Serial: 146587176140553309517047991083707763997
     * MD5 Fingerprint: 1a:79:5b:6b:04:52:9c:5d:c7:74:33:1b:25:9a:f9:25
     * SHA1 Fingerprint: 30:d4:24:6f:07:ff:db:91:89:8a:0b:e9:49:66:11:eb:8c:5e:46:e5
     * SHA256 Fingerprint: 15:d5:b8:77:46:19:ea:7d:54:ce:1c:a6:d0:b0:c4:03:e0:37:a9:17:f1:31:e8:a0:4e:1e:6b:7a:71:ba:bc:e5
     * -----BEGIN CERTIFICATE-----
     * MIICDDCCAZGgAwIBAgIQbkepx2ypcyRAiQ8DVd2NHTAKBggqhkjOPQQDAzBHMQsw
     * CQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZpY2VzIExMQzEU
     * MBIGA1UEAxMLR1RTIFJvb3QgUjMwHhcNMTYwNjIyMDAwMDAwWhcNMzYwNjIyMDAw
     * MDAwWjBHMQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZp
     * Y2VzIExMQzEUMBIGA1UEAxMLR1RTIFJvb3QgUjMwdjAQBgcqhkjOPQIBBgUrgQQA
     * IgNiAAQfTzOHMymKoYTey8chWEGJ6ladK0uFxh1MJ7x/JlFyb+Kf1qPKzEUURout
     * 736GjOyxfi//qXGdGIRFBEFVbivqJn+7kAHjSxm65FSWRQmx1WyRRK2EE46ajA2A
     * DDL24CejQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTADAQH/MB0GA1Ud
     * DgQWBBTB8Sa6oC2uhYHP0/EqEr24Cmf9vDAKBggqhkjOPQQDAwNpADBmAjEAgFuk
     * fCPAlaUs3L6JbyO5o91lAFJekazInXJ0glMLfalAvWhgxeG4VDvBNhcl2MG9AjEA
     * njWSdIUlUfUk7GRSJFClH9voy8l27OyCbvWFGFPouOOaKaqW04MjyaR7YbPMAuhd
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\"0 \x06\x03U\x04\n\x13\x19Google Trust Services LLC1\x140\x12\x06\x03U\x04\x03\x13\x0bGTS Root R3",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x1fO3\x873)\x8a\xa1\x84\xde\xcb\xc7!XA\x89\xeaV\x9d+K\x85\xc6\x1dL\'\xbc\x7f&Qro\xe2\x9f\xd6\xa3\xca\xccE\x14F\x8b\xad\xef~\x86\x8c\xec\xb1~/\xff\xa9q\x9d\x18\x84E\x04AUn+\xea&\x7f\xbb\x90\x01\xe3K\x19\xba\xe4T\x96E\t\xb1\xd5l\x91D\xad\x84\x13\x8e\x9a\x8c\r\x80\x0c2\xf6\xe0\'",
        name_constraints: None
    },

    /*
     * Issuer: CN=Network Solutions Certificate Authority O=Network Solutions L.L.C.
     * Subject: CN=Network Solutions Certificate Authority O=Network Solutions L.L.C.
     * Label: "Network Solutions Certificate Authority"
     * Serial: 116697915152937497490437556386812487904
     * MD5 Fingerprint: d3:f3:a6:16:c0:fa:6b:1d:59:b1:2d:96:4d:0e:11:2e
     * SHA1 Fingerprint: 74:f8:a3:c3:ef:e7:b3:90:06:4b:83:90:3c:21:64:60:20:e5:df:ce
     * SHA256 Fingerprint: 15:f0:ba:00:a3:ac:7a:f3:ac:88:4c:07:2b:10:11:a0:77:bd:77:c0:97:f4:01:64:b2:f8:59:8a:bd:83:86:0c
     * -----BEGIN CERTIFICATE-----
     * MIID5jCCAs6gAwIBAgIQV8szb8JcFuZHFhfjkDFo4DANBgkqhkiG9w0BAQUFADBi
     * MQswCQYDVQQGEwJVUzEhMB8GA1UEChMYTmV0d29yayBTb2x1dGlvbnMgTC5MLkMu
     * MTAwLgYDVQQDEydOZXR3b3JrIFNvbHV0aW9ucyBDZXJ0aWZpY2F0ZSBBdXRob3Jp
     * dHkwHhcNMDYxMjAxMDAwMDAwWhcNMjkxMjMxMjM1OTU5WjBiMQswCQYDVQQGEwJV
     * UzEhMB8GA1UEChMYTmV0d29yayBTb2x1dGlvbnMgTC5MLkMuMTAwLgYDVQQDEydO
     * ZXR3b3JrIFNvbHV0aW9ucyBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkwggEiMA0GCSqG
     * SIb3DQEBAQUAA4IBDwAwggEKAoIBAQDkvH6SMG3G2I4rC7xGzuAnlt7e+foS0zwz
     * c7MEL7xxjOWftiJgPl9dzgn/ggwbmlFQGiaJ3dVhXRncEg8tCqJDXRfQNJIg6nPP
     * OCwGJgl6cvf6UDL4wpPTaaIjzkGxzOTVHzbRijr4jGPiFFlp7Q3Tf2vouAPlT2rl
     * mGNpSAW+Lv8ztumXWWn4Zxmuk2GWRBXTcrA/vGp97Eh/jcOrqnErU2lBUzS1sLnF
     * BgrEsEX1QV1uiUV7PTsmjHTC5dLRfbIR1PtYMiKagMnc/Qzpf14Dl847ABSHJ3A4
     * qY5usyd2mFHgBeMhqxrVhSI8KbWaFsWAqPS7azCPL0YCorEMIuDTAgMBAAGjgZcw
     * gZQwHQYDVR0OBBYEFCEwyfsA106Y2oeqKtCnLrFAMadMMA4GA1UdDwEB/wQEAwIB
     * BjAPBgNVHRMBAf8EBTADAQH/MFIGA1UdHwRLMEkwR6BFoEOGQWh0dHA6Ly9jcmwu
     * bmV0c29sc3NsLmNvbS9OZXR3b3JrU29sdXRpb25zQ2VydGlmaWNhdGVBdXRob3Jp
     * dHkuY3JsMA0GCSqGSIb3DQEBBQUAA4IBAQC7rkvnt1frf6ott3NHhWrB5KUd5Oc8
     * 6fRZZXe1eltajSU24HqXLjjAV2CDmAaDn7l2em5Q4LqILPxFzBiwmZVRDuwduIj/
     * h1AcgsLj4DKAv6ALR8jDMe+ZZzKATxcheQxpXN5eNK4CtSbqUN9/GGUsyfJj4akH
     * /nxxH2szJGoeBfcFaMBqEssuXmHLrijTfsK0ZpEmXzwuJF/LWA/rKOyvEZbz3Htv
     * wKeI8lN3s2Berq4o2jUsbzRF0ybh3uxbTydrFny9RAQYgrOJeRcQcT16ohZO9QHN
     * pGxlaKFJdlxDydi8NmdspZS11My5vWo1ViHe2MPr+8ukYEywVaCge1ey
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1!0\x1f\x06\x03U\x04\n\x13\x18Network Solutions L.L.C.100.\x06\x03U\x04\x03\x13\'Network Solutions Certificate Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xe4\xbc~\x920m\xc6\xd8\x8e+\x0b\xbcF\xce\xe0\'\x96\xde\xde\xf9\xfa\x12\xd3<3s\xb3\x04/\xbcq\x8c\xe5\x9f\xb6\"`>_]\xce\t\xff\x82\x0c\x1b\x9aQP\x1a&\x89\xdd\xd5a]\x19\xdc\x12\x0f-\n\xa2C]\x17\xd04\x92 \xeas\xcf8,\x06&\tzr\xf7\xfaP2\xf8\xc2\x93\xd3i\xa2#\xceA\xb1\xcc\xe4\xd5\x1f6\xd1\x8a:\xf8\x8cc\xe2\x14Yi\xed\r\xd3\x7fk\xe8\xb8\x03\xe5Oj\xe5\x98ciH\x05\xbe.\xff3\xb6\xe9\x97Yi\xf8g\x19\xae\x93a\x96D\x15\xd3r\xb0?\xbcj}\xecH\x7f\x8d\xc3\xab\xaaq+SiAS4\xb5\xb0\xb9\xc5\x06\n\xc4\xb0E\xf5A]n\x89E{=;&\x8ct\xc2\xe5\xd2\xd1}\xb2\x11\xd4\xfbX2\"\x9a\x80\xc9\xdc\xfd\x0c\xe9\x7f^\x03\x97\xce;\x00\x14\x87\'p8\xa9\x8en\xb3\'v\x98Q\xe0\x05\xe3!\xab\x1a\xd5\x85\"<)\xb5\x9a\x16\xc5\x80\xa8\xf4\xbbk0\x8f/F\x02\xa2\xb1\x0c\"\xe0\xd3\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Baltimore CyberTrust Root O=Baltimore OU=CyberTrust
     * Subject: CN=Baltimore CyberTrust Root O=Baltimore OU=CyberTrust
     * Label: "Baltimore CyberTrust Root"
     * Serial: 33554617
     * MD5 Fingerprint: ac:b6:94:a5:9c:17:e0:d7:91:52:9b:b1:97:06:a6:e4
     * SHA1 Fingerprint: d4:de:20:d0:5e:66:fc:53:fe:1a:50:88:2c:78:db:28:52:ca:e4:74
     * SHA256 Fingerprint: 16:af:57:a9:f6:76:b0:ab:12:60:95:aa:5e:ba:de:f2:2a:b3:11:19:d6:44:ac:95:cd:4b:93:db:f3:f2:6a:eb
     * -----BEGIN CERTIFICATE-----
     * MIIDdzCCAl+gAwIBAgIEAgAAuTANBgkqhkiG9w0BAQUFADBaMQswCQYDVQQGEwJJ
     * RTESMBAGA1UEChMJQmFsdGltb3JlMRMwEQYDVQQLEwpDeWJlclRydXN0MSIwIAYD
     * VQQDExlCYWx0aW1vcmUgQ3liZXJUcnVzdCBSb290MB4XDTAwMDUxMjE4NDYwMFoX
     * DTI1MDUxMjIzNTkwMFowWjELMAkGA1UEBhMCSUUxEjAQBgNVBAoTCUJhbHRpbW9y
     * ZTETMBEGA1UECxMKQ3liZXJUcnVzdDEiMCAGA1UEAxMZQmFsdGltb3JlIEN5YmVy
     * VHJ1c3QgUm9vdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKMEuyKr
     * mD1X6CZymrV51Cni4eiVgLGw41uOKymaZN+hXe2wCQVt2yguzmKiYv60iNoS6zjr
     * IZ3AQSsBUnuId9Mcj8e6uYi1agnnc+gRQKfRzMpijS3ljwumUNKoUMMo6vWrJYeK
     * mpYcqWe4PwzV9/lSEy/CG9VwcPCPwBLKBsua4dnKM3p31vjsufFoREJIE9LAwqSu
     * XmD+tqYF/LTdB1kC1FkYmGP1pWPgkAx9XbIGevOF6uvUA65ehD5f/xXtabz5OTZy
     * dc93Uk3zyZAsuT3lySNTPx8kmCFcB5kpvcY67Oduhjprl3RjM71oGDHweI12v/ye
     * jl0qhqdNkNwnGjkCAwEAAaNFMEMwHQYDVR0OBBYEFOWdWTCCR1jMrPoIVDaGezq1
     * BE3wMBIGA1UdEwEB/wQIMAYBAf8CAQMwDgYDVR0PAQH/BAQDAgEGMA0GCSqGSIb3
     * DQEBBQUAA4IBAQCFDF2O5G9RaEIFoN27TyclhAO992T9Ldcw46QQF+vaKSm2eT92
     * 9hkTI7gQCvlYpNRhcL0EYWoSihfVCr3FvDB81ukMJY2GQE/szKN+OMY3EU/t3Wgx
     * jkzSswF07r51XgdIGn9w/xZchMB5hbgF/X++ZRGjD8ACtPhSNzkE1akxehi/oCr0
     * Epn3o0WC4zxe9Z2etciefC7IpJ5OCBRLbf1wbWsaY71k5h+3zvDyny67G7fyUIhz
     * ksLi4xaNmjICq44Y3ekQEe5+NauQrz4wlHrQMz2nZQ/1/I6eYs9HRCwBXbsdtTLS
     * R9I4LtD+gdwyah617jzV/OeBHRnDJELqYzmp
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02IE1\x120\x10\x06\x03U\x04\n\x13\tBaltimore1\x130\x11\x06\x03U\x04\x0b\x13\nCyberTrust1\"0 \x06\x03U\x04\x03\x13\x19Baltimore CyberTrust Root",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xa3\x04\xbb\"\xab\x98=W\xe8&r\x9a\xb5y\xd4)\xe2\xe1\xe8\x95\x80\xb1\xb0\xe3[\x8e+)\x9ad\xdf\xa1]\xed\xb0\t\x05m\xdb(.\xceb\xa2b\xfe\xb4\x88\xda\x12\xeb8\xeb!\x9d\xc0A+\x01R{\x88w\xd3\x1c\x8f\xc7\xba\xb9\x88\xb5j\t\xe7s\xe8\x11@\xa7\xd1\xcc\xcab\x8d-\xe5\x8f\x0b\xa6P\xd2\xa8P\xc3(\xea\xf5\xab%\x87\x8a\x9a\x96\x1c\xa9g\xb8?\x0c\xd5\xf7\xf9R\x13/\xc2\x1b\xd5pp\xf0\x8f\xc0\x12\xca\x06\xcb\x9a\xe1\xd9\xca3zw\xd6\xf8\xec\xb9\xf1hDBH\x13\xd2\xc0\xc2\xa4\xae^`\xfe\xb6\xa6\x05\xfc\xb4\xdd\x07Y\x02\xd4Y\x18\x98c\xf5\xa5c\xe0\x90\x0c}]\xb2\x06z\xf3\x85\xea\xeb\xd4\x03\xae^\x84>_\xff\x15\xedi\xbc\xf996ru\xcfwRM\xf3\xc9\x90,\xb9=\xe5\xc9#S?\x1f$\x98!\\\x07\x99)\xbd\xc6:\xec\xe7n\x86:k\x97tc3\xbdh\x181\xf0x\x8dv\xbf\xfc\x9e\x8e]*\x86\xa7M\x90\xdc\'\x1a9\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=COMODO ECC Certification Authority O=COMODO CA Limited
     * Subject: CN=COMODO ECC Certification Authority O=COMODO CA Limited
     * Label: "COMODO ECC Certification Authority"
     * Serial: 41578283867086692638256921589707938090
     * MD5 Fingerprint: 7c:62:ff:74:9d:31:53:5e:68:4a:d5:78:aa:1e:bf:23
     * SHA1 Fingerprint: 9f:74:4e:9f:2b:4d:ba:ec:0f:31:2c:50:b6:56:3b:8e:2d:93:c3:11
     * SHA256 Fingerprint: 17:93:92:7a:06:14:54:97:89:ad:ce:2f:8f:34:f7:f0:b6:6d:0f:3a:e3:a3:b8:4d:21:ec:15:db:ba:4f:ad:c7
     * -----BEGIN CERTIFICATE-----
     * MIICiTCCAg+gAwIBAgIQH0evqmIAcFBUTAGem2OZKjAKBggqhkjOPQQDAzCBhTEL
     * MAkGA1UEBhMCR0IxGzAZBgNVBAgTEkdyZWF0ZXIgTWFuY2hlc3RlcjEQMA4GA1UE
     * BxMHU2FsZm9yZDEaMBgGA1UEChMRQ09NT0RPIENBIExpbWl0ZWQxKzApBgNVBAMT
     * IkNPTU9ETyBFQ0MgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMDgwMzA2MDAw
     * MDAwWhcNMzgwMTE4MjM1OTU5WjCBhTELMAkGA1UEBhMCR0IxGzAZBgNVBAgTEkdy
     * ZWF0ZXIgTWFuY2hlc3RlcjEQMA4GA1UEBxMHU2FsZm9yZDEaMBgGA1UEChMRQ09N
     * T0RPIENBIExpbWl0ZWQxKzApBgNVBAMTIkNPTU9ETyBFQ0MgQ2VydGlmaWNhdGlv
     * biBBdXRob3JpdHkwdjAQBgcqhkjOPQIBBgUrgQQAIgNiAAQDR3svdcmCFYX7deSR
     * FtSrYpn1PlILBs5BAH+X4QokPB0BBO490o0JlwzgdeT6+3eKKvUDYEs2ixYjFq0J
     * cfRK9ChQtP6IHG4/bC8vCVlbpVsLM5niwz2J+Wos77LTBumjQjBAMB0GA1UdDgQW
     * BBR1cacZSBm8nZ3qQUfflMRId5nTeTAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/
     * BAUwAwEB/zAKBggqhkjOPQQDAwNoADBlAjEA7wNbeqy3eApyt4jf/7VGFAkK+qDm
     * fQjGGoe9GKhzvSbKYAydzpmfz1wPMOG+FDHqAjAU9JM8SaczepBGR7NjfRObTrdv
     * GDeAU/7dIOA1mjbRxwG55tzd8/8dLDoWV9mSOdY=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GB1\x1b0\x19\x06\x03U\x04\x08\x13\x12Greater Manchester1\x100\x0e\x06\x03U\x04\x07\x13\x07Salford1\x1a0\x18\x06\x03U\x04\n\x13\x11COMODO CA Limited1+0)\x06\x03U\x04\x03\x13\"COMODO ECC Certification Authority",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x03G{/u\xc9\x82\x15\x85\xfbu\xe4\x91\x16\xd4\xabb\x99\xf5>R\x0b\x06\xceA\x00\x7f\x97\xe1\n$<\x1d\x01\x04\xee=\xd2\x8d\t\x97\x0c\xe0u\xe4\xfa\xfbw\x8a*\xf5\x03`K6\x8b\x16#\x16\xad\tq\xf4J\xf4(P\xb4\xfe\x88\x1cn?l//\tY[\xa5[\x0b3\x99\xe2\xc3=\x89\xf9j,\xef\xb2\xd3\x06\xe9",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign O=GlobalSign OU=GlobalSign ECC Root CA - R5
     * Subject: CN=GlobalSign O=GlobalSign OU=GlobalSign ECC Root CA - R5
     * Label: "GlobalSign ECC Root CA - R5"
     * Serial: 32785792099990507226680698011560947931244
     * MD5 Fingerprint: 9f:ad:3b:1c:02:1e:8a:ba:17:74:38:81:0c:a2:bc:08
     * SHA1 Fingerprint: 1f:24:c6:30:cd:a4:18:ef:20:69:ff:ad:4f:dd:5f:46:3a:1b:69:aa
     * SHA256 Fingerprint: 17:9f:bc:14:8a:3d:d0:0f:d2:4e:a1:34:58:cc:43:bf:a7:f5:9c:81:82:d7:83:a5:13:f6:eb:ec:10:0c:89:24
     * -----BEGIN CERTIFICATE-----
     * MIICHjCCAaSgAwIBAgIRYFlJ4CYuu1X5CneKcflK2GwwCgYIKoZIzj0EAwMwUDEk
     * MCIGA1UECxMbR2xvYmFsU2lnbiBFQ0MgUm9vdCBDQSAtIFI1MRMwEQYDVQQKEwpH
     * bG9iYWxTaWduMRMwEQYDVQQDEwpHbG9iYWxTaWduMB4XDTEyMTExMzAwMDAwMFoX
     * DTM4MDExOTAzMTQwN1owUDEkMCIGA1UECxMbR2xvYmFsU2lnbiBFQ0MgUm9vdCBD
     * QSAtIFI1MRMwEQYDVQQKEwpHbG9iYWxTaWduMRMwEQYDVQQDEwpHbG9iYWxTaWdu
     * MHYwEAYHKoZIzj0CAQYFK4EEACIDYgAER0UOlvt9Xb/pOdEh+J8LttV7HpI6SFkc
     * 8GIxLcB6KP4ap1yztsyX50XUWPrRd21DosCHZTQKH3rd6zwzocWdTaRvQZU4f8ke
     * hOvRnkmSh5SHDDqFSmafnVmTTZdhBoZKo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYD
     * VR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUPeYpSJvqB8ohREom3m7e0oPQn1kwCgYI
     * KoZIzj0EAwMDaAAwZQIxAOVpEslu28YxuglB4Zf4+/2a4n0Sye18ZNPLBSWLVtmg
     * 515dTguDnFt2KaAJJiFqYgIwcdK1j1zqO+F4CYWodZI7yFz9SO8NdCKoCOJuxUnO
     * xwy8p2Fp8fc74SrL+SvzZpA3
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1$0\"\x06\x03U\x04\x0b\x13\x1bGlobalSign ECC Root CA - R51\x130\x11\x06\x03U\x04\n\x13\nGlobalSign1\x130\x11\x06\x03U\x04\x03\x13\nGlobalSign",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04GE\x0e\x96\xfb}]\xbf\xe99\xd1!\xf8\x9f\x0b\xb6\xd5{\x1e\x92:HY\x1c\xf0b1-\xc0z(\xfe\x1a\xa7\\\xb3\xb6\xcc\x97\xe7E\xd4X\xfa\xd1wmC\xa2\xc0\x87e4\n\x1fz\xdd\xeb<3\xa1\xc5\x9dM\xa4oA\x958\x7f\xc9\x1e\x84\xeb\xd1\x9eI\x92\x87\x94\x87\x0c:\x85Jf\x9f\x9dY\x93M\x97a\x06\x86J",
        name_constraints: None
    },

    /*
     * Issuer: CN=Amazon Root CA 3 O=Amazon
     * Subject: CN=Amazon Root CA 3 O=Amazon
     * Label: "Amazon Root CA 3"
     * Serial: 143266986699090766294700635381230934788665930
     * MD5 Fingerprint: a0:d4:ef:0b:f7:b5:d8:49:95:2a:ec:f5:c4:fc:81:87
     * SHA1 Fingerprint: 0d:44:dd:8c:3c:8c:1a:1a:58:75:64:81:e9:0f:2e:2a:ff:b3:d2:6e
     * SHA256 Fingerprint: 18:ce:6c:fe:7b:f1:4e:60:b2:e3:47:b8:df:e8:68:cb:31:d0:2e:bb:3a:da:27:15:69:f5:03:43:b4:6d:b3:a4
     * -----BEGIN CERTIFICATE-----
     * MIIBtjCCAVugAwIBAgITBmyf1XSXNmY/Owua2eiedgPySjAKBggqhkjOPQQDAjA5
     * MQswCQYDVQQGEwJVUzEPMA0GA1UEChMGQW1hem9uMRkwFwYDVQQDExBBbWF6b24g
     * Um9vdCBDQSAzMB4XDTE1MDUyNjAwMDAwMFoXDTQwMDUyNjAwMDAwMFowOTELMAkG
     * A1UEBhMCVVMxDzANBgNVBAoTBkFtYXpvbjEZMBcGA1UEAxMQQW1hem9uIFJvb3Qg
     * Q0EgMzBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABCmXp8ZBf8ANm+gBG1bG8lKl
     * ui2yEujSLtf6ycXYqm0fc4E7O5hrOXwzpcVOho6AF2hiRVd9RFgdszflZwjrZt6j
     * QjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgGGMB0GA1UdDgQWBBSr
     * ttvXBp43rDCGB5Fwx5zEGbF4wDAKBggqhkjOPQQDAgNJADBGAiEA4IWSoxe3jfkr
     * BqWTrBqYaGFy+uGh0PsceGCmQ5nFuMQCIQCcAu/xlJyzlvnrxir4tiz+OpAUFteM
     * YyRIHN8wfdVoOw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0f0\r\x06\x03U\x04\n\x13\x06Amazon1\x190\x17\x06\x03U\x04\x03\x13\x10Amazon Root CA 3",
        spki: b"0\x13\x06\x07*\x86H\xce=\x02\x01\x06\x08*\x86H\xce=\x03\x01\x07\x03B\x00\x04)\x97\xa7\xc6A\x7f\xc0\r\x9b\xe8\x01\x1bV\xc6\xf2R\xa5\xba-\xb2\x12\xe8\xd2.\xd7\xfa\xc9\xc5\xd8\xaam\x1fs\x81;;\x98k9|3\xa5\xc5N\x86\x8e\x80\x17hbEW}DX\x1d\xb37\xe5g\x08\xebf\xde",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root CA 3 O=QuoVadis Limited
     * Subject: CN=QuoVadis Root CA 3 O=QuoVadis Limited
     * Label: "QuoVadis Root CA 3"
     * Serial: 1478
     * MD5 Fingerprint: 31:85:3c:62:94:97:63:b9:aa:fd:89:4e:af:6f:e0:cf
     * SHA1 Fingerprint: 1f:49:14:f7:d8:74:95:1d:dd:ae:02:c0:be:fd:3a:2d:82:75:51:85
     * SHA256 Fingerprint: 18:f1:fc:7f:20:5d:f8:ad:dd:eb:7f:e0:07:dd:57:e3:af:37:5a:9c:4d:8d:73:54:6b:f4:f1:fe:d1:e1:8d:35
     * -----BEGIN CERTIFICATE-----
     * MIIGnTCCBIWgAwIBAgICBcYwDQYJKoZIhvcNAQEFBQAwRTELMAkGA1UEBhMCQk0x
     * GTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxGzAZBgNVBAMTElF1b1ZhZGlzIFJv
     * b3QgQ0EgMzAeFw0wNjExMjQxOTExMjNaFw0zMTExMjQxOTA2NDRaMEUxCzAJBgNV
     * BAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBMaW1pdGVkMRswGQYDVQQDExJRdW9W
     * YWRpcyBSb290IENBIDMwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQDM
     * V0IWVJzmmNPTTe7+7cefQzlKZbPoFog02w1ZkXTPkrgEQK0CSzGrvI2RaNggDhoB
     * 4hp7Thdd4oq3P5kazethq8Jlph+3t723j/z9cI8LoGe+AaJZz3HmDyl2/7FWeUUr
     * H556VOijKTVopAFPD6QuN+8bv+OPEKhyq1hX51SGyMnzW9os2l2ObjyjPtr7guXd
     * 8lyyBTNvijbO0BNO/79KDDRMpsMhvVAEVeuxu537RR5kFd5VAYwCdrXLoT9Cabwv
     * vWhDFlaJKjdhkf2mrk7AyxRllDdLkgbvBNDInIjbC3uBr7E9KsRlOni27tyAsdLT
     * mZw67mtaa7ONt9XOnMK+pUsvFrGeaDsGb659n/je7Mwpp5ijJUMv7/FfJuGITfhe
     * btfZFG4ZM2mnO4SJk8RTVROhUXhA+LjJou57ulJCg54U7QVSWllWp5f8nT8KKdjc
     * T5EOE7zelaTfi5m+rJsziO+1ga8bxiJTyPbH7pcUsMV8eFLI8M5ud2CEpukqdiDt
     * WAEXMJPpGovgc2PZapKUSU60rUqFxKMiMPwJ7Wgic6aIDFUhWMXhOp8q3crhkODZ
     * c6tsgLjoC2SToJyMGf+z0gzskSaHirOi4XCPLArlzW1oUevaPwV/izLmE1xr/l9A
     * 4iLItLRkT9a6fUg+qGkM17uGcclzuD87nSVL2v9A6wIDAQABo4IBlTCCAZEwDwYD
     * VR0TAQH/BAUwAwEB/zCB4QYDVR0gBIHZMIHWMIHTBgkrBgEEAb5YAAMwgcUwgZMG
     * CCsGAQUFBwICMIGGGoGDQW55IHVzZSBvZiB0aGlzIENlcnRpZmljYXRlIGNvbnN0
     * aXR1dGVzIGFjY2VwdGFuY2Ugb2YgdGhlIFF1b1ZhZGlzIFJvb3QgQ0EgMyBDZXJ0
     * aWZpY2F0ZSBQb2xpY3kgLyBDZXJ0aWZpY2F0aW9uIFByYWN0aWNlIFN0YXRlbWVu
     * dC4wLQYIKwYBBQUHAgEWIWh0dHA6Ly93d3cucXVvdmFkaXNnbG9iYWwuY29tL2Nw
     * czALBgNVHQ8EBAMCAQYwHQYDVR0OBBYEFPLAE+CCQz777i9nMpY1XNu4ywLQMG4G
     * A1UdIwRnMGWAFPLAE+CCQz777i9nMpY1XNu4ywLQoUmkRzBFMQswCQYDVQQGEwJC
     * TTEZMBcGA1UEChMQUXVvVmFkaXMgTGltaXRlZDEbMBkGA1UEAxMSUXVvVmFkaXMg
     * Um9vdCBDQSAzggIFxjANBgkqhkiG9w0BAQUFAAOCAgEAT62gLEz6wPJv92ZVqyM0
     * 7ucp2sNbtrCD2dDQ4iH782CnO11gUyeim/YIIirnv6By5ZwkajGxkHon24QRiSem
     * d1o417+shvzuXYO8BsbRd2sPbSQvS3pspweWyuOEn62Iix2rFo1bZhfZFvSLgNLd
     * +LJ2w/w4E6oM3kJpK27zPOuAJ9v1pkQNn1pVWQvVDVJIxa6f8i+AxeoyUDUSly7B
     * 4f/xI4hROJ/yZlZ25w9Rl6VSDE1JUZU2Pb+iSwwQHYaZTKrzchGT5Or2m9qoXadN
     * t54CrnMAyNojA+j56hl0YgCUyyIgvpSnWbWCar6ZeXqp8kokUvd0/bpO5qgdAm6x
     * DYBEwa7TIzdfu4V8K5Iu6H6li92Z4b8nby1dqnuH/grdS/yO9SbkbnBCbjPsMZ57
     * k8HkyWkaPcBrTiJt7qtYTcbQQcEr6k8Sh17rRdhs9ZgC06DYVYoGmRmioHfRMJ6s
     * zHXug/WwYjnPbFfiTNKRCw51KBuav/0aQ/HKd/s7j2G4aSgWQgRecCocIdiP4b0j
     * Wy10QJLZYxkNc91pvGJHvOB0K7Lrfb5BG7XARsWhIstfTsEokt4YutUqKLsRixeT
     * mJlglFwjz1onl14LBQaTNx47aTbrqZ5hHY8y2o4M1nQ+ewkk2gF3R8Q7zTSMmfXK
     * 4SVhM7JZG+Ju1zdXtg2pEto=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1\x1b0\x19\x06\x03U\x04\x03\x13\x12QuoVadis Root CA 3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xccWB\x16T\x9c\xe6\x98\xd3\xd3M\xee\xfe\xed\xc7\x9fC9Je\xb3\xe8\x16\x884\xdb\rY\x91t\xcf\x92\xb8\x04@\xad\x02K1\xab\xbc\x8d\x91h\xd8 \x0e\x1a\x01\xe2\x1a{N\x17]\xe2\x8a\xb7?\x99\x1a\xcd\xeba\xab\xc2e\xa6\x1f\xb7\xb7\xbd\xb7\x8f\xfc\xfdp\x8f\x0b\xa0g\xbe\x01\xa2Y\xcfq\xe6\x0f)v\xff\xb1VyE+\x1f\x9ezT\xe8\xa3)5h\xa4\x01O\x0f\xa4.7\xef\x1b\xbf\xe3\x8f\x10\xa8r\xabXW\xe7T\x86\xc8\xc9\xf3[\xda,\xda]\x8en<\xa3>\xda\xfb\x82\xe5\xdd\xf2\\\xb2\x053o\x8a6\xce\xd0\x13N\xff\xbfJ\x0c4L\xa6\xc3!\xbdP\x04U\xeb\xb1\xbb\x9d\xfbE\x1ed\x15\xdeU\x01\x8c\x02v\xb5\xcb\xa1?Bi\xbc/\xbdhC\x16V\x89*7a\x91\xfd\xa6\xaeN\xc0\xcb\x14e\x947K\x92\x06\xef\x04\xd0\xc8\x9c\x88\xdb\x0b{\x81\xaf\xb1=*\xc4e:x\xb6\xee\xdc\x80\xb1\xd2\xd3\x99\x9c:\xeekZk\xb3\x8d\xb7\xd5\xce\x9c\xc2\xbe\xa5K/\x16\xb1\x9eh;\x06o\xae}\x9f\xf8\xde\xec\xcc)\xa7\x98\xa3%C/\xef\xf1_&\xe1\x88M\xf8^n\xd7\xd9\x14n\x193i\xa7;\x84\x89\x93\xc4SU\x13\xa1Qx@\xf8\xb8\xc9\xa2\xee{\xbaRB\x83\x9e\x14\xed\x05RZYV\xa7\x97\xfc\x9d?\n)\xd8\xdcO\x91\x0e\x13\xbc\xde\x95\xa4\xdf\x8b\x99\xbe\xac\x9b3\x88\xef\xb5\x81\xaf\x1b\xc6\"S\xc8\xf6\xc7\xee\x97\x14\xb0\xc5|xR\xc8\xf0\xcenw`\x84\xa6\xe9*v \xedX\x01\x170\x93\xe9\x1a\x8b\xe0sc\xd9j\x92\x94IN\xb4\xadJ\x85\xc4\xa3\"0\xfc\t\xedh\"s\xa6\x88\x0cU!X\xc5\xe1:\x9f*\xdd\xca\xe1\x90\xe0\xd9s\xabl\x80\xb8\xe8\x0bd\x93\xa0\x9c\x8c\x19\xff\xb3\xd2\x0c\xec\x91&\x87\x8a\xb3\xa2\xe1p\x8f,\n\xe5\xcdmhQ\xeb\xda?\x05\x7f\x8b2\xe6\x13\\k\xfe_@\xe2\"\xc8\xb4\xb4dO\xd6\xba}H>\xa8i\x0c\xd7\xbb\x86q\xc9s\xb8?;\x9d%K\xda\xff@\xeb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Amazon Root CA 2 O=Amazon
     * Subject: CN=Amazon Root CA 2 O=Amazon
     * Label: "Amazon Root CA 2"
     * Serial: 143266982885963551818349160658925006970653239
     * MD5 Fingerprint: c8:e5:8d:ce:a8:42:e2:7a:c0:2a:5c:7c:9e:26:bf:66
     * SHA1 Fingerprint: 5a:8c:ef:45:d7:a6:98:59:76:7a:8c:8b:44:96:b5:78:cf:47:4b:1a
     * SHA256 Fingerprint: 1b:a5:b2:aa:8c:65:40:1a:82:96:01:18:f8:0b:ec:4f:62:30:4d:83:ce:c4:71:3a:19:c3:9c:01:1e:a4:6d:b4
     * -----BEGIN CERTIFICATE-----
     * MIIFQTCCAymgAwIBAgITBmyf0pY1hp8KD+WGePhbJruKNzANBgkqhkiG9w0BAQwF
     * ADA5MQswCQYDVQQGEwJVUzEPMA0GA1UEChMGQW1hem9uMRkwFwYDVQQDExBBbWF6
     * b24gUm9vdCBDQSAyMB4XDTE1MDUyNjAwMDAwMFoXDTQwMDUyNjAwMDAwMFowOTEL
     * MAkGA1UEBhMCVVMxDzANBgNVBAoTBkFtYXpvbjEZMBcGA1UEAxMQQW1hem9uIFJv
     * b3QgQ0EgMjCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK2Wny2cSkxK
     * gXlRmeyKy2tgURO8TW0G/LAIjd0ZEGrHJgw12MBvIITplLGbhQPDW9tK6Mj4kHbZ
     * W0/jTOgGNk3Mmqw9DJArktQGGWCsN0R5hYGCrVo34A3MnaZMUnbqQ523BNFQ9lXg
     * 1dKmSYXpN+nKfq5clU1Imj+uIFptiJXZNLhSGkOQsL9sBbm2eLfq0OQ6PBJTYv9K
     * 8nu+NQWpEjTj82R0Yiw9AElaKP4yRLuH3WUnAnE72kr3H9rN9yFVkE8P7K6C4Z9r
     * 2UXTu/Bfh+08LDmG2j/e7HJV63mjrdvdfLC6HM783k81ds8P+HgfajZRRidhW+me
     * z/CiVX18JYpvL7TFz4QuK/0NURBs+18bvBt+xa47mAExkv8LV/SasrlX6avvDXbR
     * 8O70zoan4G7ptGmh32n2M8ZpLpcTnqWHsFcQgTfJU7O7f/aS0ZzQGPSSbtqDT6Zj
     * mUyl+17vIWR6IF9sZIUVyzfpYgwLKhbcAS4y2j5L9Z469hdAlO+ekQiG+r5jqFoz
     * 7Mt0Q5X5bGlSNscpb/xVA1wf+5+9R+vnSUeVC06JIglJ4PVhHvG/LopyboBZ/1c6
     * +XUyo05f7O0oYtlNc/LMgRdg7c3r3NunysV+Ar3yVAhU/bQtCSwXVEqY0VThUWcI
     * 0u1ufm8/0i2BWSlmy5A5lREedCf+3euvAgMBAAGjQjBAMA8GA1UdEwEB/wQFMAMB
     * Af8wDgYDVR0PAQH/BAQDAgGGMB0GA1UdDgQWBBSwDPBMMPQFWAJI/TPlUq9LhONm
     * UjANBgkqhkiG9w0BAQwFAAOCAgEAqqiAjw54o+Ci1M3m9Zh6O+oAA7CXDpO8Wqj2
     * LIxyh6mx/H9z/WNxeKWHWc8w4Q0QshNabYL1auaAn6AFC2jkR2vHat+2/XcycuUY
     * +gn0oJMsXdKMdYV2ZZAMA3m3MSNjrXiDCYZohMr/+c8mmpJ5581LxedhpxfL86kS
     * k5Nrp+gvU5LEYFiwzAJRGFuFjWJZY7attN6a+yb3ACfAXVU3dJnJUH/jWS5E4ywl
     * 7uxMMne0nxrpS10gxdr9HIcWxkPo1LsmmkVwXqkLN1PiRnsn/eBG8om3zEK2yygm
     * btmlyTrIQRNg91CMFa6ybRoVGld45pIq2WWQgj9sAq+uEjonljYE1x2igGOpm/Hl
     * urR8FLBOybEfdF849lHqm/osohHUqS0nGkWxr7JOcQ3AWEbWaQbLU8uz/mtBzUF+
     * fUwPfHJ5elnNXkoOrJupmHN5fLT0zLm4BwyydFy4x2+IoZCn9Kr5v2c69BoVYh63
     * n749sSmvZ6ES8lgQGVMDMBu4Gon2nL2XA46jCfMdiyHxtN/kHNGfZQIG6lzWE7OE
     * 76KlXIx3KadowGuuQNKotOrN8I1LOJwZmhsoVLiJkO/KdYE+HvJkJMcYr07/R54H
     * 9jVlpNMKVv/1F2Rs76giJUmTtt8AF9pYfl3uxRuw0dFfIRDH+fO6AgonB8Xx1sfT
     * 4PsJYGw=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0f0\r\x06\x03U\x04\n\x13\x06Amazon1\x190\x17\x06\x03U\x04\x03\x13\x10Amazon Root CA 2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xad\x96\x9f-\x9cJLJ\x81yQ\x99\xec\x8a\xcbk`Q\x13\xbcMm\x06\xfc\xb0\x08\x8d\xdd\x19\x10j\xc7&\x0c5\xd8\xc0o \x84\xe9\x94\xb1\x9b\x85\x03\xc3[\xdbJ\xe8\xc8\xf8\x90v\xd9[O\xe3L\xe8\x066M\xcc\x9a\xac=\x0c\x90+\x92\xd4\x06\x19`\xac7Dy\x85\x81\x82\xadZ7\xe0\r\xcc\x9d\xa6LRv\xeaC\x9d\xb7\x04\xd1P\xf6U\xe0\xd5\xd2\xa6I\x85\xe97\xe9\xca~\xae\\\x95MH\x9a?\xae Zm\x88\x95\xd94\xb8R\x1aC\x90\xb0\xbfl\x05\xb9\xb6x\xb7\xea\xd0\xe4:<\x12Sb\xffJ\xf2{\xbe5\x05\xa9\x124\xe3\xf3dtb,=\x00IZ(\xfe2D\xbb\x87\xdde\'\x02q;\xdaJ\xf7\x1f\xda\xcd\xf7!U\x90O\x0f\xec\xae\x82\xe1\x9fk\xd9E\xd3\xbb\xf0_\x87\xed<,9\x86\xda?\xde\xecrU\xeby\xa3\xad\xdb\xdd|\xb0\xba\x1c\xce\xfc\xdeO5v\xcf\x0f\xf8x\x1fj6QF\'a[\xe9\x9e\xcf\xf0\xa2U}|%\x8ao/\xb4\xc5\xcf\x84.+\xfd\rQ\x10l\xfb_\x1b\xbc\x1b~\xc5\xae;\x98\x011\x92\xff\x0bW\xf4\x9a\xb2\xb9W\xe9\xab\xef\rv\xd1\xf0\xee\xf4\xce\x86\xa7\xe0n\xe9\xb4i\xa1\xdfi\xf63\xc6i.\x97\x13\x9e\xa5\x87\xb0W\x10\x817\xc9S\xb3\xbb\x7f\xf6\x92\xd1\x9c\xd0\x18\xf4\x92n\xda\x83O\xa6c\x99L\xa5\xfb^\xef!dz _ld\x85\x15\xcb7\xe9b\x0c\x0b*\x16\xdc\x01.2\xda>K\xf5\x9e:\xf6\x17@\x94\xef\x9e\x91\x08\x86\xfa\xbec\xa8Z3\xec\xcbtC\x95\xf9liR6\xc7)o\xfcU\x03\\\x1f\xfb\x9f\xbdG\xeb\xe7IG\x95\x0bN\x89\"\tI\xe0\xf5a\x1e\xf1\xbf.\x8arn\x80Y\xffW:\xf9u2\xa3N_\xec\xed(b\xd9Ms\xf2\xcc\x81\x17`\xed\xcd\xeb\xdc\xdb\xa7\xca\xc5~\x02\xbd\xf2T\x08T\xfd\xb4-\t,\x17TJ\x98\xd1T\xe1Qg\x08\xd2\xedn~o?\xd2-\x81Y)f\xcb\x909\x95\x11\x1et\'\xfe\xdd\xeb\xaf\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=SSL.com EV Root Certification Authority ECC O=SSL Corporation
     * Subject: CN=SSL.com EV Root Certification Authority ECC O=SSL Corporation
     * Label: "SSL.com EV Root Certification Authority ECC"
     * Serial: 3182246526754555285
     * MD5 Fingerprint: 59:53:22:65:83:42:01:54:c0:ce:42:b9:5a:7c:f2:90
     * SHA1 Fingerprint: 4c:dd:51:a3:d1:f5:20:32:14:b0:c6:c5:32:23:03:91:c7:46:42:6d
     * SHA256 Fingerprint: 22:a2:c1:f7:bd:ed:70:4c:c1:e7:01:b5:f4:08:c3:10:88:0f:e9:56:b5:de:2a:4a:44:f9:9c:87:3a:25:a7:c8
     * -----BEGIN CERTIFICATE-----
     * MIIClDCCAhqgAwIBAgIILCmcWxbtBZUwCgYIKoZIzj0EAwIwfzELMAkGA1UEBhMC
     * VVMxDjAMBgNVBAgMBVRleGFzMRAwDgYDVQQHDAdIb3VzdG9uMRgwFgYDVQQKDA9T
     * U0wgQ29ycG9yYXRpb24xNDAyBgNVBAMMK1NTTC5jb20gRVYgUm9vdCBDZXJ0aWZp
     * Y2F0aW9uIEF1dGhvcml0eSBFQ0MwHhcNMTYwMjEyMTgxNTIzWhcNNDEwMjEyMTgx
     * NTIzWjB/MQswCQYDVQQGEwJVUzEOMAwGA1UECAwFVGV4YXMxEDAOBgNVBAcMB0hv
     * dXN0b24xGDAWBgNVBAoMD1NTTCBDb3Jwb3JhdGlvbjE0MDIGA1UEAwwrU1NMLmNv
     * bSBFViBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5IEVDQzB2MBAGByqGSM49
     * AgEGBSuBBAAiA2IABKoSR5CYG/vvw0AHgyBO8TCCogbR8pKGYfL2IWjKAMTH6kMA
     * VIbc/R/fALhBYlzccBYy3h+Z1MzFB8gIH2EWB1E9fVwHU+M1OIzfzZ/ZLg1Kthku
     * WnBaBu2+8KGwytAJKaNjMGEwHQYDVR0OBBYEFFvKXuXe0oGqzagtZFG22XKbl+ZP
     * MA8GA1UdEwEB/wQFMAMBAf8wHwYDVR0jBBgwFoAUW8pe5d7SgarNqC1kUbbZcpuX
     * 5k8wDgYDVR0PAQH/BAQDAgGGMAoGCCqGSM49BAMCA2gAMGUCMQCK5kCJN+vp1RPZ
     * ytRrJPOwPYdGWBrssd9v+1a6cGvHOMzosYxPD/fxZ3YOg9AeUY8CMD32IygmTMZg
     * h5Mmm7I1HrrW9zzRHM76JTymGoEVW/MSD2zuZYrJh6j5B+BimoxcSg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0e0\x0c\x06\x03U\x04\x08\x0c\x05Texas1\x100\x0e\x06\x03U\x04\x07\x0c\x07Houston1\x180\x16\x06\x03U\x04\n\x0c\x0fSSL Corporation1402\x06\x03U\x04\x03\x0c+SSL.com EV Root Certification Authority ECC",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xaa\x12G\x90\x98\x1b\xfb\xef\xc3@\x07\x83 N\xf10\x82\xa2\x06\xd1\xf2\x92\x86a\xf2\xf6!h\xca\x00\xc4\xc7\xeaC\x00T\x86\xdc\xfd\x1f\xdf\x00\xb8Ab\\\xdcp\x162\xde\x1f\x99\xd4\xcc\xc5\x07\xc8\x08\x1fa\x16\x07Q=}\\\x07S\xe358\x8c\xdf\xcd\x9f\xd9.\rJ\xb6\x19.ZpZ\x06\xed\xbe\xf0\xa1\xb0\xca\xd0\t)",
        name_constraints: None
    },

    /*
     * Issuer: CN=Izenpe.com O=IZENPE S.A.
     * Subject: CN=Izenpe.com O=IZENPE S.A.
     * Label: "Izenpe.com"
     * Serial: 917563065490389241595536686991402621
     * MD5 Fingerprint: a6:b0:cd:85:80:da:5c:50:34:a3:39:90:2f:55:67:73
     * SHA1 Fingerprint: 2f:78:3d:25:52:18:a7:4a:65:39:71:b5:2c:a2:9c:45:15:6f:e9:19
     * SHA256 Fingerprint: 25:30:cc:8e:98:32:15:02:ba:d9:6f:9b:1f:ba:1b:09:9e:2d:29:9e:0f:45:48:bb:91:4f:36:3b:c0:d4:53:1f
     * -----BEGIN CERTIFICATE-----
     * MIIF8TCCA9mgAwIBAgIQALC3WhZIX7/hy/WL1xnmfTANBgkqhkiG9w0BAQsFADA4
     * MQswCQYDVQQGEwJFUzEUMBIGA1UECgwLSVpFTlBFIFMuQS4xEzARBgNVBAMMCkl6
     * ZW5wZS5jb20wHhcNMDcxMjEzMTMwODI4WhcNMzcxMjEzMDgyNzI1WjA4MQswCQYD
     * VQQGEwJFUzEUMBIGA1UECgwLSVpFTlBFIFMuQS4xEzARBgNVBAMMCkl6ZW5wZS5j
     * b20wggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQDJ03rKDx6sp4boFmVq
     * scIbRTJxldn+EFvMr+eleQGPicPK8lVx93e+d5TzcqQsRNiekpsUOqHnJJAKClaO
     * xdgmlOHZSOEtPtoKct2jmRXagaKH9HtuJneJWK3W6wyyQXpzbm3benhB6QiIEn6H
     * LmYRY2xU+zydcsC8Lv/Ct90NduM61/e0aL6i9eOBbsFGb12N4E3GVFWJGjMxCrFX
     * uaOKmMPsOzTFlUFpfnXCPCDFYbpRR6AgkJOhkEvzTnyFRVSa0QUmQbC1TR0zvsQD
     * yCV8wXDbO/QJLVQnSKwv4cSsPsjLkkxTOTcj7NMB+eAJRE1NZMDhDVqHIrytG6P+
     * JrUV86f8hBnp7KGItERphIPzidF0BqnMC9bC3ieFUCbKF7jJeodWLBoBHmy+E60Q
     * rLUk9TiRodZL2vG70t5HtfG8gfZZa88ZU+mNFctKy6lvROUbQc/hhqfK0GqfvEyN
     * BjNaooXlkDWgYlwWTvDjovoDGrQscbNYLN57C9saD+veIR8GdwYDsMnvmfzAuU8L
     * hij+0rnq49qlw0dpEuDb8PYZi+17cNcC1u2HGCgsBCRMd+RIihrGO5rUD8r6ddIB
     * QFqNeb+Lz0vPqhbBleStTIo+F5HUsWLlguWABKQDfo2/2n+iD5dPDNMN+9fR5XJ+
     * HMh3/1uaD7euBUbl8agW7EekFwIDAQABo4H2MIHzMIGwBgNVHREEgagwgaWBD2lu
     * Zm9AaXplbnBlLmNvbaSBkTCBjjFHMEUGA1UECgw+SVpFTlBFIFMuQS4gLSBDSUYg
     * QTAxMzM3MjYwLVJNZXJjLlZpdG9yaWEtR2FzdGVpeiBUMTA1NSBGNjIgUzgxQzBB
     * BgNVBAkMOkF2ZGEgZGVsIE1lZGl0ZXJyYW5lbyBFdG9yYmlkZWEgMTQgLSAwMTAx
     * MCBWaXRvcmlhLUdhc3RlaXowDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMC
     * AQYwHQYDVR0OBBYEFB0cZQ6o8iV7tJHP5LGx5r1VdGwFMA0GCSqGSIb3DQEBCwUA
     * A4ICAQB4pgwWSp9MiDrAyw6lFn2fuUhfGI8NYjb2zRlrrKvV9pF9rnHzP7MOeIWb
     * laQnIUdCSnxIOvVFfLMMjlF4rJUT3sb9fbgakEyrkgPH7UIBzg/YsfqikuFgba56
     * awmqxinuaElnMIAkejEWOVt+8Rwu3WwJrfIxwYJOubv5vr8qhT/AQKM6WfxZSzwo
     * JNu0FXWuDYi6LnPAvViH5ULy617uHjAimcs30cQhbIHsvm0m5hzkQiCeR7Csg1lw
     * LDXWrzY0tM07+DKo7+N4ifuNRSzanLh+QBxh5z6ikixL8s36mLYp//Pye6kfLqCT
     * VyvehQP5aTfLnnhqBbTFMXiJ7HqnheG5ezzevh55hM6fcA5ZwjUukCox2eRFekGk
     * LhObNA5me0mrZJfQRsN5nXJQY6aYWwa9SG3YOYNw6DXwBdGqvOPbyALqfP2C2sJb
     * UjWumDqtujWTI6cfSN01RpiyEGjkpTHCClguGYEQyVB1/OpaFs4R1+7vUIgtYf8/
     * QnMFlEPVjjxOAToZpR9GTnfQXeWBIiGH/pR9hNiTrdZoQ0iy2+tzJOeRf1SktoA+
     * naM8THLCV8Sg1Mw4J87VBp6iSNnpn86CcDaTmjvfliHjWbcM2pE38P1ZWrOZyGls
     * QyYBNWNgVYkDOnXYukrZVP/u3oDYLdE41V4tC5h9Pmzb/CaIxw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02ES1\x140\x12\x06\x03U\x04\n\x0c\x0bIZENPE S.A.1\x130\x11\x06\x03U\x04\x03\x0c\nIzenpe.com",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc9\xd3z\xca\x0f\x1e\xac\xa7\x86\xe8\x16ej\xb1\xc2\x1bE2q\x95\xd9\xfe\x10[\xcc\xaf\xe7\xa5y\x01\x8f\x89\xc3\xca\xf2Uq\xf7w\xbew\x94\xf3r\xa4,D\xd8\x9e\x92\x9b\x14:\xa1\xe7$\x90\n\nV\x8e\xc5\xd8&\x94\xe1\xd9H\xe1->\xda\nr\xdd\xa3\x99\x15\xda\x81\xa2\x87\xf4{n&w\x89X\xad\xd6\xeb\x0c\xb2Azsnm\xdbzxA\xe9\x08\x88\x12~\x87.f\x11clT\xfb<\x9dr\xc0\xbc.\xff\xc2\xb7\xdd\rv\xe3:\xd7\xf7\xb4h\xbe\xa2\xf5\xe3\x81n\xc1Fo]\x8d\xe0M\xc6TU\x89\x1a31\n\xb1W\xb9\xa3\x8a\x98\xc3\xec;4\xc5\x95Ai~u\xc2< \xc5a\xbaQG\xa0 \x90\x93\xa1\x90K\xf3N|\x85ET\x9a\xd1\x05&A\xb0\xb5M\x1d3\xbe\xc4\x03\xc8%|\xc1p\xdb;\xf4\t-T\'H\xac/\xe1\xc4\xac>\xc8\xcb\x92LS97#\xec\xd3\x01\xf9\xe0\tDMMd\xc0\xe1\rZ\x87\"\xbc\xad\x1b\xa3\xfe&\xb5\x15\xf3\xa7\xfc\x84\x19\xe9\xec\xa1\x88\xb4Di\x84\x83\xf3\x89\xd1t\x06\xa9\xcc\x0b\xd6\xc2\xde\'\x85P&\xca\x17\xb8\xc9z\x87V,\x1a\x01\x1el\xbe\x13\xad\x10\xac\xb5$\xf58\x91\xa1\xd6K\xda\xf1\xbb\xd2\xdeG\xb5\xf1\xbc\x81\xf6Yk\xcf\x19S\xe9\x8d\x15\xcbJ\xcb\xa9oD\xe5\x1bA\xcf\xe1\x86\xa7\xca\xd0j\x9f\xbcL\x8d\x063Z\xa2\x85\xe5\x905\xa0b\\\x16N\xf0\xe3\xa2\xfa\x03\x1a\xb4,q\xb3X,\xde{\x0b\xdb\x1a\x0f\xeb\xde!\x1f\x06w\x06\x03\xb0\xc9\xef\x99\xfc\xc0\xb9O\x0b\x86(\xfe\xd2\xb9\xea\xe3\xda\xa5\xc3Gi\x12\xe0\xdb\xf0\xf6\x19\x8b\xed{p\xd7\x02\xd6\xed\x87\x18(,\x04$Lw\xe4H\x8a\x1a\xc6;\x9a\xd4\x0f\xca\xfau\xd2\x01@Z\x8dy\xbf\x8b\xcfK\xcf\xaa\x16\xc1\x95\xe4\xadL\x8a>\x17\x91\xd4\xb1b\xe5\x82\xe5\x80\x04\xa4\x03~\x8d\xbf\xda\x7f\xa2\x0f\x97O\x0c\xd3\r\xfb\xd7\xd1\xe5r~\x1c\xc8w\xff[\x9a\x0f\xb7\xae\x05F\xe5\xf1\xa8\x16\xecG\xa4\x17\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GTS Root R1 O=Google Trust Services LLC
     * Subject: CN=GTS Root R1 O=Google Trust Services LLC
     * Label: "GTS Root R1"
     * Serial: 146587175971765017618439757810265552097
     * MD5 Fingerprint: 82:1a:ef:d4:d2:4a:f2:9f:e2:3d:97:06:14:70:72:85
     * SHA1 Fingerprint: e1:c9:50:e6:ef:22:f8:4c:56:45:72:8b:92:20:60:d7:d5:a7:a3:e8
     * SHA256 Fingerprint: 2a:57:54:71:e3:13:40:bc:21:58:1c:bd:2c:f1:3e:15:84:63:20:3e:ce:94:bc:f9:d3:cc:19:6b:f0:9a:54:72
     * -----BEGIN CERTIFICATE-----
     * MIIFWjCCA0KgAwIBAgIQbkepxUtHDA3sM9CJuRz04TANBgkqhkiG9w0BAQwFADBH
     * MQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZpY2VzIExM
     * QzEUMBIGA1UEAxMLR1RTIFJvb3QgUjEwHhcNMTYwNjIyMDAwMDAwWhcNMzYwNjIy
     * MDAwMDAwWjBHMQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNl
     * cnZpY2VzIExMQzEUMBIGA1UEAxMLR1RTIFJvb3QgUjEwggIiMA0GCSqGSIb3DQEB
     * AQUAA4ICDwAwggIKAoICAQC2EQKLHuOhd5s73L+UPreVp0A8of2C+X0yBoJx9vaM
     * f/vo27xqLpeXo4xL+Sv2sfnOhB2x+cWX3u+58qPpvBKJXqeqUqv4IyfLpLGcY9vX
     * mX7wCl7raKb0xlpHDU0QM+NOsROjyBhsS+z8CZDfnWQpJSMHobTSPS5g4M/SCYe7
     * zUjwTcLCeoiKu7rPWRnWr4+wB7CeMfGCwcDfLqZtbBkOtdh+JhpFAz2weaSUKK0P
     * fyblqAj+lug8aJRT7oM6iCsVlgmy4HqMLnXWnOunVmSPlk9orj2XwoSPwLxAwAtc
     * vfaHszVsrBhQf4TgTM2S0yDpM7xSma8ytSmzJSq0SPly4cpk9+aCEI3oncKKiPo4
     * Zor8Y/kB+Xj9e1x3+naH+uzfsQ55lVe0vSbv1gHR6xYKu44LtcXFilWr06zqkUsp
     * zBmkMiVOKvFlRNACzqrOSbTqn3yDsEB750Orp2yjj32JgfpMpf/VjsPOS+C12LOO
     * Rc92wO1AK/1TD7Cn1TsNsYqiA94xrcx36m97PtbfkSIS5r762DL8EGMUUXLeXdYW
     * k70paDPvOmbsB4om3xPXV2V4J95eSRQAogB/mqghtqmxlbCluQ0WEdrHbEg8QOB+
     * DVrNVjzRlwW5y0vtOUucxD/SVRNuJLDWcfr0wbrM7Rv1/oFB2ACYPTrIrnqYNxgF
     * lQIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNV
     * HQ4EFgQU5K8rJnEaK0gnhS9SZizv8IkTcT4wDQYJKoZIhvcNAQEMBQADggIBADiW
     * Cu49tJYeX++dnAsznyvgyv3SjgofQXSlfKqE1OXyHuY3UjKcC9FhHb8owbZEKTV1
     * d5iyfNm9dKyKaOOpMQkpAWBz40d8U6iQSifvS9efk+eCNs6aaAyC58/UEBZvXw6Z
     * XPYfcX3v73svfuo21pdwCxXu11xWajOl40k4DLh9+42FpLFZXvRq4d2h9mREruZR
     * gyFmxhE+885H7pwoHyXa/6xmld01D1zvICxi/ZG6qcz8WpyTgYMpl0p8WnK0OdC3
     * d8t5/Wk6kjftbjhlRn7pYL15iJdfOBL07q9bgsiG1eGZbYwE8na6SfZu6W0eX6Dv
     * J4J2QPim01hcDyxC2kLGe4g0x8HYRZvBPsVhHdljUEn2NIVq4BjFbkerQUIpm/Zg
     * DdIx02OYI5NaAIFItO/Nis3Jz5nu2Z6qNuFoS3FJFDYoOj0dzpqPJeaAcWErtXvM
     * +SUWgeExX6GjfhaknBZqlxi9dnKlC54dNuYvoS++cJEPqOba+MSSQGwlfnuzCdyy
     * F62ARPBopY+Udf90WuioAnwMCeKpSwughQtiue+hMZL77/ZRBIls6Kl0obsXs7X9
     * SQ98POyDGCBDTtWTurQ0sR8WNh8M5mQ5Fkzc4P4dyKliPUDqysU0ArSuiYgzNdws
     * E3PYJ/HQcu51OyLemGhmW/HGY0dVHLqlCFF1pkgl
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\"0 \x06\x03U\x04\n\x13\x19Google Trust Services LLC1\x140\x12\x06\x03U\x04\x03\x13\x0bGTS Root R1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb6\x11\x02\x8b\x1e\xe3\xa1w\x9b;\xdc\xbf\x94>\xb7\x95\xa7@<\xa1\xfd\x82\xf9}2\x06\x82q\xf6\xf6\x8c\x7f\xfb\xe8\xdb\xbcj.\x97\x97\xa3\x8cK\xf9+\xf6\xb1\xf9\xce\x84\x1d\xb1\xf9\xc5\x97\xde\xef\xb9\xf2\xa3\xe9\xbc\x12\x89^\xa7\xaaR\xab\xf8#\'\xcb\xa4\xb1\x9cc\xdb\xd7\x99~\xf0\n^\xebh\xa6\xf4\xc6ZG\rM\x103\xe3N\xb1\x13\xa3\xc8\x18lK\xec\xfc\t\x90\xdf\x9dd)%#\x07\xa1\xb4\xd2=.`\xe0\xcf\xd2\t\x87\xbb\xcdH\xf0M\xc2\xc2z\x88\x8a\xbb\xba\xcfY\x19\xd6\xaf\x8f\xb0\x07\xb0\x9e1\xf1\x82\xc1\xc0\xdf.\xa6ml\x19\x0e\xb5\xd8~&\x1aE\x03=\xb0y\xa4\x94(\xad\x0f\x7f&\xe5\xa8\x08\xfe\x96\xe8<h\x94S\xee\x83:\x88+\x15\x96\t\xb2\xe0z\x8c.u\xd6\x9c\xeb\xa7Vd\x8f\x96Oh\xae=\x97\xc2\x84\x8f\xc0\xbc@\xc0\x0b\\\xbd\xf6\x87\xb35l\xac\x18P\x7f\x84\xe0L\xcd\x92\xd3 \xe93\xbcR\x99\xaf2\xb5)\xb3%*\xb4H\xf9r\xe1\xcad\xf7\xe6\x82\x10\x8d\xe8\x9d\xc2\x8a\x88\xfa8f\x8a\xfcc\xf9\x01\xf9x\xfd{\\w\xfav\x87\xfa\xec\xdf\xb1\x0ey\x95W\xb4\xbd&\xef\xd6\x01\xd1\xeb\x16\n\xbb\x8e\x0b\xb5\xc5\xc5\x8aU\xab\xd3\xac\xea\x91K)\xcc\x19\xa42%N*\xf1eD\xd0\x02\xce\xaa\xceI\xb4\xea\x9f|\x83\xb0@{\xe7C\xab\xa7l\xa3\x8f}\x89\x81\xfaL\xa5\xff\xd5\x8e\xc3\xceK\xe0\xb5\xd8\xb3\x8eE\xcfv\xc0\xed@+\xfdS\x0f\xb0\xa7\xd5;\r\xb1\x8a\xa2\x03\xde1\xad\xccw\xeao{>\xd6\xdf\x91\"\x12\xe6\xbe\xfa\xd82\xfc\x10c\x14Qr\xde]\xd6\x16\x93\xbd)h3\xef:f\xec\x07\x8a&\xdf\x13\xd7Wex\'\xde^I\x14\x00\xa2\x00\x7f\x9a\xa8!\xb6\xa9\xb1\x95\xb0\xa5\xb9\r\x16\x11\xda\xc7lH<@\xe0~\rZ\xcdV<\xd1\x97\x05\xb9\xcbK\xed9K\x9c\xc4?\xd2U\x13n$\xb0\xd6q\xfa\xf4\xc1\xba\xcc\xed\x1b\xf5\xfe\x81A\xd8\x00\x98=:\xc8\xaez\x987\x18\x05\x95\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R6
     * Subject: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R6
     * Label: "GlobalSign Root CA - R6"
     * Serial: 1417766617973444989252670301619537
     * MD5 Fingerprint: 4f:dd:07:e4:d4:22:64:39:1e:0c:37:42:ea:d1:c6:ae
     * SHA1 Fingerprint: 80:94:64:0e:b5:a7:a1:ca:11:9c:1f:dd:d5:9f:81:02:63:a7:fb:d1
     * SHA256 Fingerprint: 2c:ab:ea:fe:37:d0:6c:a2:2a:ba:73:91:c0:03:3d:25:98:29:52:c4:53:64:73:49:76:3a:3a:b5:ad:6c:cf:69
     * -----BEGIN CERTIFICATE-----
     * MIIFgzCCA2ugAwIBAgIORea7A4Mzw4VlSOb/RVEwDQYJKoZIhvcNAQEMBQAwTDEg
     * MB4GA1UECxMXR2xvYmFsU2lnbiBSb290IENBIC0gUjYxEzARBgNVBAoTCkdsb2Jh
     * bFNpZ24xEzARBgNVBAMTCkdsb2JhbFNpZ24wHhcNMTQxMjEwMDAwMDAwWhcNMzQx
     * MjEwMDAwMDAwWjBMMSAwHgYDVQQLExdHbG9iYWxTaWduIFJvb3QgQ0EgLSBSNjET
     * MBEGA1UEChMKR2xvYmFsU2lnbjETMBEGA1UEAxMKR2xvYmFsU2lnbjCCAiIwDQYJ
     * KoZIhvcNAQEBBQADggIPADCCAgoCggIBAJUH6HPKZvnsFMp7PPcNCPG0RQssgrRI
     * xutbPK6DuEGSMxSkb3/pKszGsIhrxbaJ0cay/xTOURQh7ErdG1rG1ofuTToVBu1k
     * ZguSgMpE3nOUTvOniX9PeGMIyBJQbUJmL025eShNUhqKGoC3GYEOfsSKvGRMIRxD
     * aNc9PIrFsmbVkJq3MQbFvuJtMgamHvm566qjuL++gmNQ0PAYid/kD3n16qIfKtJw
     * LnvnvJO7bVPiSHyMEAc4/2ayd2F+4OqMPKq0pPbzlUoSB239jLKJz9CgYXfIWHSw
     * 1CM69106yqLbnQneXUQtkPGBzVeS+n68UARjNN9rkxi+azayOeSsJDa38O+2HBNX
     * k7besvjihbdzorg1qkXy4J02oW9UivFyVm4uiMVRQkQVlO6jxTiWm05OWgtH8wY2
     * SXcwvHE35absIQh1/OZhFj931dmRl4QKbNQCTXTAFO39OfuD8l4UoQSwC+n+7o/h
     * bguyCLNhZglqsQY6ZZZZwPA1/cnaKI0aEYdwgQqomnUdnjqGBQCe24DWJfncBZ4n
     * WUx2OVvq+aWh2IMP0f/fMBH5hc8zSPXKbWQULHpYT9NLCEnFlWQaYw55PfWzjMpY
     * rZxCRXluDocZXFSxZba/jJvcE+kNb7gu3GduyYsRtYQUigAZcIN5kZeR1Bonvzce
     * MgfYFGM8KEyvAgMBAAGjYzBhMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTAD
     * AQH/MB0GA1UdDgQWBBSubAWjkxPioufi1xzWx/B/yGdToDAfBgNVHSMEGDAWgBSu
     * bAWjkxPioufi1xzWx/B/yGdToDANBgkqhkiG9w0BAQwFAAOCAgEAgyXt6NH9lVLN
     * nsAEoJFp5lzQhN7craJP6Ed41mWYqVuoPId8AorRbrcWc+ZfwFSY1XS+wc3iEZGt
     * Ixg93eFyRJa0lV7Ae46ZeBZDE1ZXs6KzO7V33EByrKPrmzU+sQghoefEQzd5Mr61
     * 55wsTLxDKZmOMNOsIeDjHfrYBzN2VAAiKrlNIC5waNrlU/yDXNOd8v9EDERm8tLj
     * vUYAGm0CuiVdjaExUd1URhxN25mW7xocBFymFe944Hn+Xds+qkxV/ZoVqW/hpvvf
     * cDDpw+5CRu3CkwWJ+n1jez/QcYF8AOiYrg54NMMl+68KnyBr3TsTjxKM4kEaSHpz
     * oHdpx7Zcf4LIHv5YGygrqGytXm3ABdJ7t+uA/iU3/gKbaKxCXcPu9czc8FB10jZp
     * nOZ7BN9uBmm23goJSFmH63sUYHpkqmlD75HHTOwY3WzvUy2MmeFe8nI+z1TIvWfs
     * pA9MRf/TuTAjB0yPEL+GltmZWrSZVxykzLsViVO6LAUP5MSeGbEYNNVMnbrt9x+v
     * JJUEeKgDu+6B5dpffItKoZB0JaezPkvILFa9x8jvOOJckvB595yEunQtYQEgfn7R
     * 8k8HWV+LLUNS60YMlOH1Zkd5d9VUWx+tJDfLRVpOoERIyNiwmcUVhAn21klJwGW4
     * 5hpxbqCo8YLoRT5s1gLXCmeDBVrJpBA=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1 0\x1e\x06\x03U\x04\x0b\x13\x17GlobalSign Root CA - R61\x130\x11\x06\x03U\x04\n\x13\nGlobalSign1\x130\x11\x06\x03U\x04\x03\x13\nGlobalSign",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x95\x07\xe8s\xcaf\xf9\xec\x14\xca{<\xf7\r\x08\xf1\xb4E\x0b,\x82\xb4H\xc6\xeb[<\xae\x83\xb8A\x923\x14\xa4o\x7f\xe9*\xcc\xc6\xb0\x88k\xc5\xb6\x89\xd1\xc6\xb2\xff\x14\xceQ\x14!\xecJ\xdd\x1bZ\xc6\xd6\x87\xeeM:\x15\x06\xeddf\x0b\x92\x80\xcaD\xdes\x94N\xf3\xa7\x89\x7fOxc\x08\xc8\x12PmBf/M\xb9y(MR\x1a\x8a\x1a\x80\xb7\x19\x81\x0e~\xc4\x8a\xbcdL!\x1cCh\xd7=<\x8a\xc5\xb2f\xd5\x90\x9a\xb71\x06\xc5\xbe\xe2m2\x06\xa6\x1e\xf9\xb9\xeb\xaa\xa3\xb8\xbf\xbe\x82cP\xd0\xf0\x18\x89\xdf\xe4\x0fy\xf5\xea\xa2\x1f*\xd2p.{\xe7\xbc\x93\xbbmS\xe2H|\x8c\x10\x078\xfff\xb2wa~\xe0\xea\x8c<\xaa\xb4\xa4\xf6\xf3\x95J\x12\x07m\xfd\x8c\xb2\x89\xcf\xd0\xa0aw\xc8Xt\xb0\xd4#:\xf7]:\xca\xa2\xdb\x9d\t\xde]D-\x90\xf1\x81\xcdW\x92\xfa~\xbcP\x04c4\xdfk\x93\x18\xbek6\xb29\xe4\xac$6\xb7\xf0\xef\xb6\x1c\x13W\x93\xb6\xde\xb2\xf8\xe2\x85\xb7s\xa2\xb85\xaaE\xf2\xe0\x9d6\xa1oT\x8a\xf1rVn.\x88\xc5QBD\x15\x94\xee\xa3\xc58\x96\x9bNNZ\x0bG\xf3\x066Iw0\xbcq7\xe5\xa6\xec!\x08u\xfc\xe6a\x16?w\xd5\xd9\x91\x97\x84\nl\xd4\x02Mt\xc0\x14\xed\xfd9\xfb\x83\xf2^\x14\xa1\x04\xb0\x0b\xe9\xfe\xee\x8f\xe1n\x0b\xb2\x08\xb3af\tj\xb1\x06:e\x96Y\xc0\xf05\xfd\xc9\xda(\x8d\x1a\x11\x87p\x81\n\xa8\x9au\x1d\x9e:\x86\x05\x00\x9e\xdb\x80\xd6%\xf9\xdc\x05\x9e\'YLv9[\xea\xf9\xa5\xa1\xd8\x83\x0f\xd1\xff\xdf0\x11\xf9\x85\xcf3H\xf5\xcamd\x14,zXO\xd3K\x08I\xc5\x95d\x1ac\x0ey=\xf5\xb3\x8c\xcaX\xad\x9cBEyn\x0e\x87\x19\\T\xb1e\xb6\xbf\x8c\x9b\xdc\x13\xe9\ro\xb8.\xdcgn\xc9\x8b\x11\xb5\x84\x14\x8a\x00\x19p\x83y\x91\x97\x91\xd4\x1a\'\xbf7\x1e2\x07\xd8\x14c<(L\xaf\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Starfield Root Certificate Authority - G2 O=Starfield Technologies, Inc.
     * Subject: CN=Starfield Root Certificate Authority - G2 O=Starfield Technologies, Inc.
     * Label: "Starfield Root Certificate Authority - G2"
     * Serial: 0
     * MD5 Fingerprint: d6:39:81:c6:52:7e:96:69:fc:fc:ca:66:ed:05:f2:96
     * SHA1 Fingerprint: b5:1c:06:7c:ee:2b:0c:3d:f8:55:ab:2d:92:f4:fe:39:d4:e7:0f:0e
     * SHA256 Fingerprint: 2c:e1:cb:0b:f9:d2:f9:e1:02:99:3f:be:21:51:52:c3:b2:dd:0c:ab:de:1c:68:e5:31:9b:83:91:54:db:b7:f5
     * -----BEGIN CERTIFICATE-----
     * MIID3TCCAsWgAwIBAgIBADANBgkqhkiG9w0BAQsFADCBjzELMAkGA1UEBhMCVVMx
     * EDAOBgNVBAgTB0FyaXpvbmExEzARBgNVBAcTClNjb3R0c2RhbGUxJTAjBgNVBAoT
     * HFN0YXJmaWVsZCBUZWNobm9sb2dpZXMsIEluYy4xMjAwBgNVBAMTKVN0YXJmaWVs
     * ZCBSb290IENlcnRpZmljYXRlIEF1dGhvcml0eSAtIEcyMB4XDTA5MDkwMTAwMDAw
     * MFoXDTM3MTIzMTIzNTk1OVowgY8xCzAJBgNVBAYTAlVTMRAwDgYDVQQIEwdBcml6
     * b25hMRMwEQYDVQQHEwpTY290dHNkYWxlMSUwIwYDVQQKExxTdGFyZmllbGQgVGVj
     * aG5vbG9naWVzLCBJbmMuMTIwMAYDVQQDEylTdGFyZmllbGQgUm9vdCBDZXJ0aWZp
     * Y2F0ZSBBdXRob3JpdHkgLSBHMjCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoC
     * ggEBAL3twQP89o/8ArFvW59I2Z154qK3A2FWGMNHttfKPTUuiUP3oWmb3ooa/RMg
     * nLRJdzIpVv257IzdIvpy3Cdhl+72WoTsbhm5iSzchFvVdPtrX8WJpRBSiUZV9Lh1
     * HOZ/5FSuS/hVclcCGfgXcVnrHigHdMWdSL5stPSksPNkN3mSwOxGXn/hbVNMYq/N
     * Hwtjuzqd+/x5AJhhdM8mgkBj87JyahkNmcrUDnXMN/uLicFZ8WJ/X7NfZTD4p7dN
     * dloedl40wOiWVpmKs/B/pM293DIxfJHP4F8R+GuqSVzRmZTRouNjWwl2tVZi4Ut0
     * HZbUJtQIBFnQmA4O5t78w+wfkPECAwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAO
     * BgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFHwMMh+n2TB/xH1oo2Kooc6rB1snMA0G
     * CSqGSIb3DQEBCwUAA4IBAQARWfolTwNvlJk7mh+ChTnUdgWUXuEok21iXQnCoKjU
     * sHU48TRqneSfioYmUeYs0cYtbpUgSpIB7LiKZ3sx4mcujJUDJi5DnUox9g61DLu3
     * 4jd/IroAow57UvtruzvE03lRTs2Q9GcHGcg8RnoNAX3FWOdt5oUwF5okxBDgBPfg
     * 8n/Uqgr/Qh037ZTlZFkSIHc40zI+OIF1lnP6aI+xy84fxez6nH7PfrHxBy22/L/K
     * pL/QlwVKvOoYKAKQvVR4CSFx09F9HdkWsKlhPdAKACL8x3vLCWRFCztAgfd9fDL1
     * mMpYjn0q7pBZc2T5NnReJaH1ZgUufzkVqSr7UIuOhWn0
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x100\x0e\x06\x03U\x04\x08\x13\x07Arizona1\x130\x11\x06\x03U\x04\x07\x13\nScottsdale1%0#\x06\x03U\x04\n\x13\x1cStarfield Technologies, Inc.1200\x06\x03U\x04\x03\x13)Starfield Root Certificate Authority - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbd\xed\xc1\x03\xfc\xf6\x8f\xfc\x02\xb1o[\x9fH\xd9\x9dy\xe2\xa2\xb7\x03aV\x18\xc3G\xb6\xd7\xca=5.\x89C\xf7\xa1i\x9b\xde\x8a\x1a\xfd\x13 \x9c\xb4Iw2)V\xfd\xb9\xec\x8c\xdd\"\xfar\xdc\'a\x97\xee\xf6Z\x84\xecn\x19\xb9\x89,\xdc\x84[\xd5t\xfbk_\xc5\x89\xa5\x10R\x89FU\xf4\xb8u\x1c\xe6\x7f\xe4T\xaeK\xf8UrW\x02\x19\xf8\x17qY\xeb\x1e(\x07t\xc5\x9dH\xbel\xb4\xf4\xa4\xb0\xf3d7y\x92\xc0\xecF^\x7f\xe1mSLb\xaf\xcd\x1f\x0bc\xbb:\x9d\xfb\xfcy\x00\x98at\xcf&\x82@c\xf3\xb2rj\x19\r\x99\xca\xd4\x0eu\xcc7\xfb\x8b\x89\xc1Y\xf1b\x7f_\xb3_e0\xf8\xa7\xb7MvZ\x1ev^4\xc0\xe8\x96V\x99\x8a\xb3\xf0\x7f\xa4\xcd\xbd\xdc21|\x91\xcf\xe0_\x11\xf8k\xaaI\\\xd1\x99\x94\xd1\xa2\xe3c[\tv\xb5Vb\xe1Kt\x1d\x96\xd4&\xd4\x08\x04Y\xd0\x98\x0e\x0e\xe6\xde\xfc\xc3\xec\x1f\x90\xf1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=SSL.com EV Root Certification Authority RSA R2 O=SSL Corporation
     * Subject: CN=SSL.com EV Root Certification Authority RSA R2 O=SSL Corporation
     * Label: "SSL.com EV Root Certification Authority RSA R2"
     * Serial: 6248227494352943350
     * MD5 Fingerprint: e1:1e:31:58:1a:ae:54:53:02:f6:17:6a:11:7b:4d:95
     * SHA1 Fingerprint: 74:3a:f0:52:9b:d0:32:a0:f4:4a:83:cd:d4:ba:a9:7b:7c:2e:c4:9a
     * SHA256 Fingerprint: 2e:7b:f1:6c:c2:24:85:a7:bb:e2:aa:86:96:75:07:61:b0:ae:39:be:3b:2f:e9:d0:cc:6d:4e:f7:34:91:42:5c
     * -----BEGIN CERTIFICATE-----
     * MIIF6zCCA9OgAwIBAgIIVrYpzTS8ePYwDQYJKoZIhvcNAQELBQAwgYIxCzAJBgNV
     * BAYTAlVTMQ4wDAYDVQQIDAVUZXhhczEQMA4GA1UEBwwHSG91c3RvbjEYMBYGA1UE
     * CgwPU1NMIENvcnBvcmF0aW9uMTcwNQYDVQQDDC5TU0wuY29tIEVWIFJvb3QgQ2Vy
     * dGlmaWNhdGlvbiBBdXRob3JpdHkgUlNBIFIyMB4XDTE3MDUzMTE4MTQzN1oXDTQy
     * MDUzMDE4MTQzN1owgYIxCzAJBgNVBAYTAlVTMQ4wDAYDVQQIDAVUZXhhczEQMA4G
     * A1UEBwwHSG91c3RvbjEYMBYGA1UECgwPU1NMIENvcnBvcmF0aW9uMTcwNQYDVQQD
     * DC5TU0wuY29tIEVWIFJvb3QgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkgUlNBIFIy
     * MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAjzZlQOHWTcDXtOlG2mvq
     * M0fNTPl9fb69LT3w23jhhqXZuglXaO1XPqDQCEGD5yhBJB/jchXQARr7XnAjssuf
     * OePPxU7Gkm0mxnu7s9onnQqG6YE3Bf7wcXHswxzpY6IXFJ3vG2fThVUCAtZJycxa
     * 4bH3bzKfydQ7iEGonL3Lq9ttewkfokxykNorCPzPPFTOZw+oz12WGQvE43LrrdF9
     * HSfvkusQv1vrO6/PgN3B0pYEW3p+pKk8OHakYo6gOV7qd89dAFmPZiw+B6KjBSYR
     * aZfqhbcPlgtLyEDhULouisv3D5oi53+aNxPN8k0TayHRwMwi8qFG9kRpnMphNQcA
     * b9ZhCBHqurj26bNg5U257J8UZslXWNvNh2n4ioYSA0e/ZhN2rHd9NCSFg83XqpyQ
     * Gp8hLH94t2S42Oim9HizVcuE0jLEeK6jj2HdzghTreyI/BXkmg3mnxp3zkyPuBQV
     * PWKchjgGAGYS5Fl2WlPAApiiECtoRHuOec4zSnaqW4EWG7WK2NAAe15itAnWhmMO
     * pgWVSbooi4iTsjQc2KRVbrcc0N6ZVTsj9CLg+SlmJuwgUHfbSguPvuUCYHBBXtSu
     * UDkiFCbLsjtzdFVHB3mBOagwE0TlBIqulhMlQg+5U8Sb/M3kHN48+qvWBkofZ6aY
     * MBzdLNvcGJVXZsb/XItW9XcCAwEAAaNjMGEwDwYDVR0TAQH/BAUwAwEB/zAfBgNV
     * HSMEGDAWgBT5YLvU49U09rj1BoAlp3PbRmmonjAdBgNVHQ4EFgQU+WC71OPVNPa4
     * 9QaAJadz20ZpqJ4wDgYDVR0PAQH/BAQDAgGGMA0GCSqGSIb3DQEBCwUAA4ICAQBW
     * s47LCp1Jjr+kxJG7ZhcFUZh1++VQLHqe8RT6q9OKPv+RKY9ji9i0qVQBDb6Thi/5
     * Sm3HXvVX+cpVHBK+Rw82xd9qt9t1wkclf7nxY/hoLVUE0fKNsKTPvDxeH3jnpaAg
     * cLAExbf3cqfeIg29MyVGjGSSJuM+LmOW2puMPfgYCdcDzH2GguDKBAdRUNf/ktUM
     * 79qGn5nX67evaOI5JpS6aLe/g9Pqemc9YmeuJeVy6OLk7K4S9ksrPJ/psEDzOFSz
     * /bdoyNrGj1E8svuR3Bznm53htw1yj+KkxKl4+esUrMZDBcJlOSgYAsOCsp0FvmXt
     * ll9ldDz7CTUue5wT/RsPXcdtgTpWD8w74a8CLyKsRspGPKAcTNZEtF4uXBVmCeEm
     * Kf7GUmG6sXP/wwyc5WxqlD8UykAWlYTzWamsX0xhk23RO8yilQwipmdnRC652dKK
     * QbNmC1r7fSOl8hqw/96bg5Qu0T/fkreRrwU7ZcegbLHNYhLDkBvjJc40vG93drEQ
     * w/cFGsDWr3RiSBd3kmmQYRzelYB0VI8YHMPzA9C/pEN1hlMYegouCRw2n5H9gooi
     * S9EOUCXdywMMF8mDAAhONU2Ki+3wApRmLER/y5UnlhetCTCstnEXbosX9hwJ1C07
     * mKVx01QT2WDz9UtmT/rx7iASjbSsV7FFY6GsdqnC+w==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0e0\x0c\x06\x03U\x04\x08\x0c\x05Texas1\x100\x0e\x06\x03U\x04\x07\x0c\x07Houston1\x180\x16\x06\x03U\x04\n\x0c\x0fSSL Corporation1705\x06\x03U\x04\x03\x0c.SSL.com EV Root Certification Authority RSA R2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x8f6e@\xe1\xd6M\xc0\xd7\xb4\xe9F\xdak\xea3G\xcdL\xf9}}\xbe\xbd-=\xf0\xdbx\xe1\x86\xa5\xd9\xba\tWh\xedW>\xa0\xd0\x08A\x83\xe7(A$\x1f\xe3r\x15\xd0\x01\x1a\xfb^p#\xb2\xcb\x9f9\xe3\xcf\xc5N\xc6\x92m&\xc6{\xbb\xb3\xda\'\x9d\n\x86\xe9\x817\x05\xfe\xf0qq\xec\xc3\x1c\xe9c\xa2\x17\x14\x9d\xef\x1bg\xd3\x85U\x02\x02\xd6I\xc9\xccZ\xe1\xb1\xf7o2\x9f\xc9\xd4;\x88A\xa8\x9c\xbd\xcb\xab\xdbm{\t\x1f\xa2Lr\x90\xda+\x08\xfc\xcf<T\xceg\x0f\xa8\xcf]\x96\x19\x0b\xc4\xe3r\xeb\xad\xd1}\x1d\'\xef\x92\xeb\x10\xbf[\xeb;\xaf\xcf\x80\xdd\xc1\xd2\x96\x04[z~\xa4\xa9<8v\xa4b\x8e\xa09^\xeaw\xcf]\x00Y\x8ff,>\x07\xa2\xa3\x05&\x11i\x97\xea\x85\xb7\x0f\x96\x0bK\xc8@\xe1P\xba.\x8a\xcb\xf7\x0f\x9a\"\xe7\x7f\x9a7\x13\xcd\xf2M\x13k!\xd1\xc0\xcc\"\xf2\xa1F\xf6Di\x9c\xcaa5\x07\x00o\xd6a\x08\x11\xea\xba\xb8\xf6\xe9\xb3`\xe5M\xb9\xec\x9f\x14f\xc9WX\xdb\xcd\x87i\xf8\x8a\x86\x12\x03G\xbff\x13v\xacw}4$\x85\x83\xcd\xd7\xaa\x9c\x90\x1a\x9f!,\x7fx\xb7d\xb8\xd8\xe8\xa6\xf4x\xb3U\xcb\x84\xd22\xc4x\xae\xa3\x8fa\xdd\xce\x08S\xad\xec\x88\xfc\x15\xe4\x9a\r\xe6\x9f\x1aw\xceL\x8f\xb8\x14\x15=b\x9c\x868\x06\x00f\x12\xe4YvZS\xc0\x02\x98\xa2\x10+hD{\x8ey\xce3Jv\xaa[\x81\x16\x1b\xb5\x8a\xd8\xd0\x00{^b\xb4\t\xd6\x86c\x0e\xa6\x05\x95I\xba(\x8b\x88\x93\xb24\x1c\xd8\xa4Un\xb7\x1c\xd0\xde\x99U;#\xf4\"\xe0\xf9)f&\xec Pw\xdbJ\x0b\x8f\xbe\xe5\x02`pA^\xd4\xaeP9\"\x14&\xcb\xb2;stUG\x07y\x819\xa80\x13D\xe5\x04\x8a\xae\x96\x13%B\x0f\xb9S\xc4\x9b\xfc\xcd\xe4\x1c\xde<\xfa\xab\xd6\x06J\x1fg\xa6\x980\x1c\xdd,\xdb\xdc\x18\x95Wf\xc6\xff\\\x8bV\xf5w\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=IdenTrust Public Sector Root CA 1 O=IdenTrust
     * Subject: CN=IdenTrust Public Sector Root CA 1 O=IdenTrust
     * Label: "IdenTrust Public Sector Root CA 1"
     * Serial: 13298821034946342390521976156843933698
     * MD5 Fingerprint: 37:06:a5:b0:fc:89:9d:ba:f4:6b:8c:1a:64:cd:d5:ba
     * SHA1 Fingerprint: ba:29:41:60:77:98:3f:f4:f3:ef:f2:31:05:3b:2e:ea:6d:4d:45:fd
     * SHA256 Fingerprint: 30:d0:89:5a:9a:44:8a:26:20:91:63:55:22:d1:f5:20:10:b5:86:7a:ca:e1:2c:78:ef:95:8f:d4:f4:38:9f:2f
     * -----BEGIN CERTIFICATE-----
     * MIIFZjCCA06gAwIBAgIQCgFCgAAAAUUjz0Z8AAAAAjANBgkqhkiG9w0BAQsFADBN
     * MQswCQYDVQQGEwJVUzESMBAGA1UEChMJSWRlblRydXN0MSowKAYDVQQDEyFJZGVu
     * VHJ1c3QgUHVibGljIFNlY3RvciBSb290IENBIDEwHhcNMTQwMTE2MTc1MzMyWhcN
     * MzQwMTE2MTc1MzMyWjBNMQswCQYDVQQGEwJVUzESMBAGA1UEChMJSWRlblRydXN0
     * MSowKAYDVQQDEyFJZGVuVHJ1c3QgUHVibGljIFNlY3RvciBSb290IENBIDEwggIi
     * MA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQC2IpT8pEiv6EdrCvsnduTyP4o7
     * ekosMSqMjbCpwzFrqHd2hCa2rIFCDQjrVVi7evi8ZX3yoG2LqEfpYnYeEe4IFNGy
     * RBb06tD6Hi9e28tzQa68ALBKK0CyrOE7S8ItneShm+waOh7wCLPQ5CQ1B5+ctMlS
     * bdsHyo+1W/CD80/HLaXIrcuVIKQxKFdYWuSNG5qrng0M8gozOSI5Cpcu81N3uURF
     * /YTLNiCBWS2ab21ISGHKTN9T0a9SvESfqy9rg3LvdYDaBjMbXcjaY8ZNzaxmMc3R
     * 3j6HEDbhuaR672BQssvKplbgN6+rNBM5Jeg5ZuSYeqoSmJxZZoY+rfGwyj4GD3vw
     * EUs3oERte8uojHH01bWRNszwFcYr3lEXsZdMUD2xlVl8BX0tIdUAvwFnol57plzy
     * 9yLxkA2T26pEUWbMfXYD62qoKjgZl3YNa4ph+bz27nb9cCvdKTz4Ch5bQhyLVi9V
     * GxyhLrXHFub4qjySjmm2AcG1hp2JDws4lFTo6tyePSW8Uybt1as5qsVATFSrsrTZ
     * 2fjXctscvG29ZV/viDUqZi/u9rNl8DONfJhBaUYPQxxp+pu10GFqzcpL2UyQRqsV
     * WaFHVCkugyhfHMKiq3IXAAaOReyL4jM9f9oZRORicsPfIsbyVtTdX5Vy7W1f90gD
     * W/3FKqD2cyOEEBsB5wIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/
     * BAUwAwEB/zAdBgNVHQ4EFgQU43HgntinQtnbcZFrlJPrw6PRFKMwDQYJKoZIhvcN
     * AQELBQADggIBAEf63QqwEZE4rU1d9+UOl1QZgkiHVIyqZJnYWv6IAcVYpZmxI1Qj
     * t2odIFflAWJBF9MJ23XLblSQdf4an4EKwt3X9wnQW3IV5B4Jaj0z8yGa5hV+rVHV
     * DRDtfULAj+7AmgjVQdZcDiFpboBhDhXAuM/FSRJSzL46zNQuOAXeNf0fb7iAaJg9
     * TaDKQGXSc3z1i9kKlT/YPyNtGtEqJBnZhbMX73huqVjRI9PHE+1yJX9dsXNw0H8G
     * lwmEKYBhHfpe/3OsoOOJuBxxFcbeMX8S3OFtm6/n6J91eEyrRjuazr8FGF1NFTwW
     * mhlQBJqymm9li1JfPFgEKCXAZmExfrngdbkaqIHWchezxQMxNRF4eKLg6TCMf4Df
     * WN88uieW4oA0beOY02QnrEh+KHdcxiVhJfiFDGX6xDIvpZgF5PgLZxYWxoK4Mhn5
     * +bl53B/N66+rDt0b20XkeucC4pVd/GnwU2lhlXV5C15V5jgclKlZM57IcXR5f1GJ
     * tshquDDIajjDbp7hNxbqBWJMWxJH7ae0s1hWx0nzfxJoCTFx8G34Tkf71oXuxVhA
     * GaQdp/lLQzfcaFpPz+vCZHTetBXZ9FRUGi8c15dxVJCO2SCdUyt/q4/i6jC8UDfv
     * 8Ue1fXwsBOxonbRJRBD0ckscZOf85muQ3Wl9af0AVqW3rLatt8o+Ae+c
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x120\x10\x06\x03U\x04\n\x13\tIdenTrust1*0(\x06\x03U\x04\x03\x13!IdenTrust Public Sector Root CA 1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb6\"\x94\xfc\xa4H\xaf\xe8Gk\n\xfb\'v\xe4\xf2?\x8a;zJ,1*\x8c\x8d\xb0\xa9\xc31k\xa8wv\x84&\xb6\xac\x81B\r\x08\xebUX\xbbz\xf8\xbce}\xf2\xa0m\x8b\xa8G\xe9bv\x1e\x11\xee\x08\x14\xd1\xb2D\x16\xf4\xea\xd0\xfa\x1e/^\xdb\xcbsA\xae\xbc\x00\xb0J+@\xb2\xac\xe1;K\xc2-\x9d\xe4\xa1\x9b\xec\x1a:\x1e\xf0\x08\xb3\xd0\xe4$5\x07\x9f\x9c\xb4\xc9Rm\xdb\x07\xca\x8f\xb5[\xf0\x83\xf3O\xc7-\xa5\xc8\xad\xcb\x95 \xa41(WXZ\xe4\x8d\x1b\x9a\xab\x9e\r\x0c\xf2\n39\"9\n\x97.\xf3Sw\xb9DE\xfd\x84\xcb6 \x81Y-\x9aomHHa\xcaL\xdfS\xd1\xafR\xbcD\x9f\xab/k\x83r\xefu\x80\xda\x063\x1b]\xc8\xdac\xc6M\xcd\xacf1\xcd\xd1\xde>\x87\x106\xe1\xb9\xa4z\xef`P\xb2\xcb\xca\xa6V\xe07\xaf\xab4\x139%\xe89f\xe4\x98z\xaa\x12\x98\x9cYf\x86>\xad\xf1\xb0\xca>\x06\x0f{\xf0\x11K7\xa0Dm{\xcb\xa8\x8cq\xf4\xd5\xb5\x916\xcc\xf0\x15\xc6+\xdeQ\x17\xb1\x97LP=\xb1\x95Y|\x05}-!\xd5\x00\xbf\x01g\xa2^{\xa6\\\xf2\xf7\"\xf1\x90\r\x93\xdb\xaaDQf\xcc}v\x03\xebj\xa8*8\x19\x97v\rk\x8aa\xf9\xbc\xf6\xeev\xfdp+\xdd)<\xf8\n\x1e[B\x1c\x8bV/U\x1b\x1c\xa1.\xb5\xc7\x16\xe6\xf8\xaa<\x92\x8ei\xb6\x01\xc1\xb5\x86\x9d\x89\x0f\x0b8\x94T\xe8\xea\xdc\x9e=%\xbcS&\xed\xd5\xab9\xaa\xc5@LT\xab\xb2\xb4\xd9\xd9\xf8\xd7r\xdb\x1c\xbcm\xbde_\xef\x885*f/\xee\xf6\xb3e\xf03\x8d|\x98AiF\x0fC\x1ci\xfa\x9b\xb5\xd0aj\xcd\xcaK\xd9L\x90F\xab\x15Y\xa1GT).\x83(_\x1c\xc2\xa2\xabr\x17\x00\x06\x8eE\xec\x8b\xe23=\x7f\xda\x19D\xe4br\xc3\xdf\"\xc6\xf2V\xd4\xdd_\x95r\xedm_\xf7H\x03[\xfd\xc5*\xa0\xf6s#\x84\x10\x1b\x01\xe7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Global Root G3 O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Global Root G3 O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Global Root G3"
     * Serial: 7089244469030293291760083333884364146
     * MD5 Fingerprint: f5:5d:a4:50:a5:fb:28:7e:1e:0f:0d:cc:96:57:56:ca
     * SHA1 Fingerprint: 7e:04:de:89:6a:3e:66:6d:00:e6:87:d3:3f:fa:d9:3b:e8:3d:34:9e
     * SHA256 Fingerprint: 31:ad:66:48:f8:10:41:38:c7:38:f3:9e:a4:32:01:33:39:3e:3a:18:cc:02:29:6e:f9:7c:2a:c9:ef:67:31:d0
     * -----BEGIN CERTIFICATE-----
     * MIICPzCCAcWgAwIBAgIQBVVWvPJepDU1w6QP1atFcjAKBggqhkjOPQQDAzBhMQsw
     * CQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cu
     * ZGlnaWNlcnQuY29tMSAwHgYDVQQDExdEaWdpQ2VydCBHbG9iYWwgUm9vdCBHMzAe
     * Fw0xMzA4MDExMjAwMDBaFw0zODAxMTUxMjAwMDBaMGExCzAJBgNVBAYTAlVTMRUw
     * EwYDVQQKEwxEaWdpQ2VydCBJbmMxGTAXBgNVBAsTEHd3dy5kaWdpY2VydC5jb20x
     * IDAeBgNVBAMTF0RpZ2lDZXJ0IEdsb2JhbCBSb290IEczMHYwEAYHKoZIzj0CAQYF
     * K4EEACIDYgAE3afZu4q4C/sLfyHS8L6+c/MzXRq8NOrexpu80JX28MzQC7phW1FG
     * fp4tn+6OYwwX7Adw9c+ELkCDnOg/QW07rdOkFFk2eJ0DQ+4QE2xy3q6Ip6FrtUPO
     * Z9wj/wMco+I+o0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBhjAd
     * BgNVHQ4EFgQUs9tIpPmhxdiuNkHMEWNpYim8S8YwCgYIKoZIzj0EAwMDaAAwZQIx
     * AK288mw/EkrRLTnDCgmXc/SINoyIJ7vmiI1Qhadj+Z4y3maTD/HMsQmP3Wyr+mt/
     * oAIwOWZbwmSNuJ5Q3KjVSaLtx9zRSX8XAbjIho9OjIgrqJqpisXRAL34VOKa5Vt8
     * sycX
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1 0\x1e\x06\x03U\x04\x03\x13\x17DigiCert Global Root G3",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xdd\xa7\xd9\xbb\x8a\xb8\x0b\xfb\x0b\x7f!\xd2\xf0\xbe\xbes\xf33]\x1a\xbc4\xea\xde\xc6\x9b\xbc\xd0\x95\xf6\xf0\xcc\xd0\x0b\xbaa[QF~\x9e-\x9f\xee\x8ec\x0c\x17\xec\x07p\xf5\xcf\x84.@\x83\x9c\xe8?Am;\xad\xd3\xa4\x14Y6x\x9d\x03C\xee\x10\x13lr\xde\xae\x88\xa7\xa1k\xb5C\xceg\xdc#\xff\x03\x1c\xa3\xe2>",
        name_constraints: None
    },

    /*
     * Issuer: CN=SSL.com Root Certification Authority ECC O=SSL Corporation
     * Subject: CN=SSL.com Root Certification Authority ECC O=SSL Corporation
     * Label: "SSL.com Root Certification Authority ECC"
     * Serial: 8495723813297216424
     * MD5 Fingerprint: 2e:da:e4:39:7f:9c:8f:37:d1:70:9f:26:17:51:3a:8e
     * SHA1 Fingerprint: c3:19:7c:39:24:e6:54:af:1b:c4:ab:20:95:7a:e2:c3:0e:13:02:6a
     * SHA256 Fingerprint: 34:17:bb:06:cc:60:07:da:1b:96:1c:92:0b:8a:b4:ce:3f:ad:82:0e:4a:a3:0b:9a:cb:c4:a7:4e:bd:ce:bc:65
     * -----BEGIN CERTIFICATE-----
     * MIICjTCCAhSgAwIBAgIIdebfy8FoW6gwCgYIKoZIzj0EAwIwfDELMAkGA1UEBhMC
     * VVMxDjAMBgNVBAgMBVRleGFzMRAwDgYDVQQHDAdIb3VzdG9uMRgwFgYDVQQKDA9T
     * U0wgQ29ycG9yYXRpb24xMTAvBgNVBAMMKFNTTC5jb20gUm9vdCBDZXJ0aWZpY2F0
     * aW9uIEF1dGhvcml0eSBFQ0MwHhcNMTYwMjEyMTgxNDAzWhcNNDEwMjEyMTgxNDAz
     * WjB8MQswCQYDVQQGEwJVUzEOMAwGA1UECAwFVGV4YXMxEDAOBgNVBAcMB0hvdXN0
     * b24xGDAWBgNVBAoMD1NTTCBDb3Jwb3JhdGlvbjExMC8GA1UEAwwoU1NMLmNvbSBS
     * b290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5IEVDQzB2MBAGByqGSM49AgEGBSuB
     * BAAiA2IABEVuqVDEpiM2nl8ojRfLliJkP9x6jh3MCLOicSS6jkm5BBtHllirLZXI
     * 7Z4INcgn64mMU1jrYor+8FsPazFSY0E7ic3s7LaNGdM0B9y7xgZ/wkWV7Mt/qCPg
     * CemB+vNH06NjMGEwHQYDVR0OBBYEFILRhXMw5zUE044CkvvlpNHEIejNMA8GA1Ud
     * EwEB/wQFMAMBAf8wHwYDVR0jBBgwFoAUgtGFczDnNQTTjgKS++Wk0cQh6M0wDgYD
     * VR0PAQH/BAQDAgGGMAoGCCqGSM49BAMCA2cAMGQCMG/n61kRpGDPYbCWe+0F+S8T
     * kdzt5fxQaxFGRrMcIQBiu77D5+jNB5n5DQtdcj7EqgIwH7y6C+IwJPt8bYBVCpk+
     * gA0z5Wajs6O7pdWLjwkspl1+4vAHCGht0nxpbl/f5Wpl
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0e0\x0c\x06\x03U\x04\x08\x0c\x05Texas1\x100\x0e\x06\x03U\x04\x07\x0c\x07Houston1\x180\x16\x06\x03U\x04\n\x0c\x0fSSL Corporation110/\x06\x03U\x04\x03\x0c(SSL.com Root Certification Authority ECC",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04En\xa9P\xc4\xa6#6\x9e_(\x8d\x17\xcb\x96\"d?\xdcz\x8e\x1d\xcc\x08\xb3\xa2q$\xba\x8eI\xb9\x04\x1bG\x96X\xab-\x95\xc8\xed\x9e\x085\xc8\'\xeb\x89\x8cSX\xebb\x8a\xfe\xf0[\x0fk1RcA;\x89\xcd\xec\xec\xb6\x8d\x19\xd34\x07\xdc\xbb\xc6\x06\x7f\xc2E\x95\xec\xcb\x7f\xa8#\xe0\t\xe9\x81\xfa\xf3G\xd3",
        name_constraints: None
    },

    /*
     * Issuer: CN=Microsoft ECC Root Certificate Authority 2017 O=Microsoft Corporation
     * Subject: CN=Microsoft ECC Root Certificate Authority 2017 O=Microsoft Corporation
     * Label: "Microsoft ECC Root Certificate Authority 2017"
     * Serial: 136839042543790627607696632466672567020
     * MD5 Fingerprint: dd:a1:03:e6:4a:93:10:d1:bf:f0:19:42:cb:fe:ed:67
     * SHA1 Fingerprint: 99:9a:64:c3:7f:f4:7d:9f:ab:95:f1:47:69:89:14:60:ee:c4:c3:c5
     * SHA256 Fingerprint: 35:8d:f3:9d:76:4a:f9:e1:b7:66:e9:c9:72:df:35:2e:e1:5c:fa:c2:27:af:6a:d1:d7:0e:8e:4a:6e:dc:ba:02
     * -----BEGIN CERTIFICATE-----
     * MIICWTCCAd+gAwIBAgIQZvI9r4fei7FK6gxXMQHC7DAKBggqhkjOPQQDAzBlMQsw
     * CQYDVQQGEwJVUzEeMBwGA1UEChMVTWljcm9zb2Z0IENvcnBvcmF0aW9uMTYwNAYD
     * VQQDEy1NaWNyb3NvZnQgRUNDIFJvb3QgQ2VydGlmaWNhdGUgQXV0aG9yaXR5IDIw
     * MTcwHhcNMTkxMjE4MjMwNjQ1WhcNNDIwNzE4MjMxNjA0WjBlMQswCQYDVQQGEwJV
     * UzEeMBwGA1UEChMVTWljcm9zb2Z0IENvcnBvcmF0aW9uMTYwNAYDVQQDEy1NaWNy
     * b3NvZnQgRUNDIFJvb3QgQ2VydGlmaWNhdGUgQXV0aG9yaXR5IDIwMTcwdjAQBgcq
     * hkjOPQIBBgUrgQQAIgNiAATUvD0CQnVBEyPNgASGAlEvaqiBYgtlzPbKnR5vSmZR
     * ogPZnZH6thaxjG7efM3beaYvzrvOcS/lpaso7GMEZpn4+vKTEAXhgShC48Zo9OYb
     * hGBKia/teQ87zvH2RPUBeMCjVDBSMA4GA1UdDwEB/wQEAwIBhjAPBgNVHRMBAf8E
     * BTADAQH/MB0GA1UdDgQWBBTIy5lycFIM+Oa+sgRXKSrPQhDtNTAQBgkrBgEEAYI3
     * FQEEAwIBADAKBggqhkjOPQQDAwNoADBlAjBY8k3qDPlfXu5gKcs68tvWMoQZP3zV
     * L8KxzJOuULsJMsbG7X7JNpQS5GiFBqIb0C8CMQCZ6Ra0DvpWSNSkMBaReNtUjGUB
     * iudQZsIxtzm6uBoiB078a1QWIP8rtedMDE2mT3M=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x1e0\x1c\x06\x03U\x04\n\x13\x15Microsoft Corporation1604\x06\x03U\x04\x03\x13-Microsoft ECC Root Certificate Authority 2017",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xd4\xbc=\x02BuA\x13#\xcd\x80\x04\x86\x02Q/j\xa8\x81b\x0be\xcc\xf6\xca\x9d\x1eoJfQ\xa2\x03\xd9\x9d\x91\xfa\xb6\x16\xb1\x8cn\xde|\xcd\xdby\xa6/\xce\xbb\xceq/\xe5\xa5\xab(\xecc\x04f\x99\xf8\xfa\xf2\x93\x10\x05\xe1\x81(B\xe3\xc6h\xf4\xe6\x1b\x84`J\x89\xaf\xedy\x0f;\xce\xf1\xf6D\xf5\x01x\xc0",
        name_constraints: None
    },

    /*
     * Issuer: CN=Microsec e-Szigno Root CA 2009 O=Microsec Ltd.
     * Subject: CN=Microsec e-Szigno Root CA 2009 O=Microsec Ltd.
     * Label: "Microsec e-Szigno Root CA 2009"
     * Serial: 14014712776195784473
     * MD5 Fingerprint: f8:49:f4:03:bc:44:2d:83:be:48:69:7d:29:64:fc:b1
     * SHA1 Fingerprint: 89:df:74:fe:5c:f4:0f:4a:80:f9:e3:37:7d:54:da:91:e1:01:31:8e
     * SHA256 Fingerprint: 3c:5f:81:fe:a5:fa:b8:2c:64:bf:a2:ea:ec:af:cd:e8:e0:77:fc:86:20:a7:ca:e5:37:16:3d:f3:6e:db:f3:78
     * -----BEGIN CERTIFICATE-----
     * MIIECjCCAvKgAwIBAgIJAMJ+QwRORz8ZMA0GCSqGSIb3DQEBCwUAMIGCMQswCQYD
     * VQQGEwJIVTERMA8GA1UEBwwIQnVkYXBlc3QxFjAUBgNVBAoMDU1pY3Jvc2VjIEx0
     * ZC4xJzAlBgNVBAMMHk1pY3Jvc2VjIGUtU3ppZ25vIFJvb3QgQ0EgMjAwOTEfMB0G
     * CSqGSIb3DQEJARYQaW5mb0BlLXN6aWduby5odTAeFw0wOTA2MTYxMTMwMThaFw0y
     * OTEyMzAxMTMwMThaMIGCMQswCQYDVQQGEwJIVTERMA8GA1UEBwwIQnVkYXBlc3Qx
     * FjAUBgNVBAoMDU1pY3Jvc2VjIEx0ZC4xJzAlBgNVBAMMHk1pY3Jvc2VjIGUtU3pp
     * Z25vIFJvb3QgQ0EgMjAwOTEfMB0GCSqGSIb3DQEJARYQaW5mb0BlLXN6aWduby5o
     * dTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAOn4j/NjrdqG2KfgQvvP
     * kd6mJviZpWNwrZuuyjNAfW2WbqEORO7hE52UQlKavXWFdCyoDh2Tthi3jCyoz/tc
     * cbna7P7ofo/kLx2yqHWH2Leh5TvPmUpG0IMZfcChEhyVbUr02MelTTMuhTlAdX4U
     * fIASmFDHQWe4oIBhVKZsTh/gnQ4H6cm6M+f+wFUoLAKApxn1ntxVUwOXewdI/5n7
     * N4okxFnMUBBjjqqpGrCEGob5X7uxUG6k0QrM1XF+H6cbfPVTbiJfyyvm1HxdrtbC
     * xkzlBQHZ7Vf8wSN5/PrIJIOV87VqUQHQd9bpEqH5GoP7ghu5sJf0dgYzQ0mg/wu1
     * +rUCAwEAAaOBgDB+MA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMB0G
     * A1UdDgQWBBTLD8bfQkPMPcu1SCOhGnqmKrs0aDAfBgNVHSMEGDAWgBTLD8bfQkPM
     * Pcu1SCOhGnqmKrs0aDAbBgNVHREEFDASgRBpbmZvQGUtc3ppZ25vLmh1MA0GCSqG
     * SIb3DQEBCwUAA4IBAQDJ0Q5eLtXMs3w+y/w9/w0olZMEyL/azXm4Q5DwpL7v8u8h
     * mLzU1F0G9u5C7DBsoKqpyvGvivo/C3NqPuouQH4frlRheesuCDfXI/OMn74dseGk
     * ddug4lQUsbocKaQY9hK6ohQU4zE1yED/t+AFdlfBHFny+L/k7SViXITwfn4fs775
     * tyERzAMBVnCnEJIeGzSBHq2cGsMEPO0CYdYeBvNfOofyK/FFh+U9rNHHV4S9a67c
     * 2Pm2G2JwCz02yULyMtd6YebS2z3PyKnJm9zbWETXbzivf3jTo60adbocwTZ8jx5t
     * HMN1Rq41Bab2XD0h7lbwyYIiLXpUq3DDfSJlgnCW
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02HU1\x110\x0f\x06\x03U\x04\x07\x0c\x08Budapest1\x160\x14\x06\x03U\x04\n\x0c\rMicrosec Ltd.1\'0%\x06\x03U\x04\x03\x0c\x1eMicrosec e-Szigno Root CA 20091\x1f0\x1d\x06\t*\x86H\x86\xf7\r\x01\t\x01\x16\x10info@e-szigno.hu",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xe9\xf8\x8f\xf3c\xad\xda\x86\xd8\xa7\xe0B\xfb\xcf\x91\xde\xa6&\xf8\x99\xa5cp\xad\x9b\xae\xca3@}m\x96n\xa1\x0eD\xee\xe1\x13\x9d\x94BR\x9a\xbdu\x85t,\xa8\x0e\x1d\x93\xb6\x18\xb7\x8c,\xa8\xcf\xfb\\q\xb9\xda\xec\xfe\xe8~\x8f\xe4/\x1d\xb2\xa8u\x87\xd8\xb7\xa1\xe5;\xcf\x99JF\xd0\x83\x19}\xc0\xa1\x12\x1c\x95mJ\xf4\xd8\xc7\xa5M3.\x859@u~\x14|\x80\x12\x98P\xc7Ag\xb8\xa0\x80aT\xa6lN\x1f\xe0\x9d\x0e\x07\xe9\xc9\xba3\xe7\xfe\xc0U(,\x02\x80\xa7\x19\xf5\x9e\xdcUS\x03\x97{\x07H\xff\x99\xfb7\x8a$\xc4Y\xccP\x10c\x8e\xaa\xa9\x1a\xb0\x84\x1a\x86\xf9_\xbb\xb1Pn\xa4\xd1\n\xcc\xd5q~\x1f\xa7\x1b|\xf5Sn\"_\xcb+\xe6\xd4|]\xae\xd6\xc2\xc6L\xe5\x05\x01\xd9\xedW\xfc\xc1#y\xfc\xfa\xc8$\x83\x95\xf3\xb5jQ\x01\xd0w\xd6\xe9\x12\xa1\xf9\x1a\x83\xfb\x82\x1b\xb9\xb0\x97\xf4v\x063CI\xa0\xff\x0b\xb5\xfa\xb5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Assured ID Root CA O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Assured ID Root CA O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Assured ID Root CA"
     * Serial: 17154717934120587862167794914071425081
     * MD5 Fingerprint: 87:ce:0b:7b:2a:0e:49:00:e1:58:71:9b:37:a8:93:72
     * SHA1 Fingerprint: 05:63:b8:63:0d:62:d7:5a:bb:c8:ab:1e:4b:df:b5:a8:99:b2:4d:43
     * SHA256 Fingerprint: 3e:90:99:b5:01:5e:8f:48:6c:00:bc:ea:9d:11:1e:e7:21:fa:ba:35:5a:89:bc:f1:df:69:56:1e:3d:c6:32:5c
     * -----BEGIN CERTIFICATE-----
     * MIIDtzCCAp+gAwIBAgIQDOfg5RfYRv6P5WD8G/AwOTANBgkqhkiG9w0BAQUFADBl
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJv
     * b3QgQ0EwHhcNMDYxMTEwMDAwMDAwWhcNMzExMTEwMDAwMDAwWjBlMQswCQYDVQQG
     * EwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cuZGlnaWNl
     * cnQuY29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJvb3QgQ0EwggEi
     * MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCtDhXO5EOAXLGH87dg+XESpa7c
     * JpSIqvTO9SA5KFhgDPiA2qkVlTJhPLWxKISKityfCgyDF3qPkKyK53lTXDGEKvYP
     * mDI2dsze3Tyoou9q+yHyUmHfnyDXH+Kx2f4YZNISW1/5WBg1vEfNoTb5a3/UsDg+
     * wRvDjDPZ2C8Y/igPs6eD1sNuRMBhNZYW/lmci3Zt1/GiSw0r/wty2p5g0I6QNcZ4
     * VYcgoc/lbQrISXwxmDNsIumH0DJaoroTghHtORedmTpyoeb6pNnVFzF1roV9Iq4/
     * AUaG9ih5yLHa5FcXxH4cDrC0kqZWs72yl+2qp/C3xag/lRbQ/6GW6whfGHdPAgMB
     * AAGjYzBhMA4GA1UdDwEB/wQEAwIBhjAPBgNVHRMBAf8EBTADAQH/MB0GA1UdDgQW
     * BBRF66Kv9JLLgjEtUYunpyGd823IDzAfBgNVHSMEGDAWgBRF66Kv9JLLgjEtUYun
     * pyGd823IDzANBgkqhkiG9w0BAQUFAAOCAQEAog683+Lt8ONyc3pklL/3cmbYMuRC
     * dWKuh+vy1dneVrOfzM4UKLkNl2BcEkxY5NM9g0lFWJc1aRqoR+pWxnmrEthngYTf
     * fwk8lOa4JiwgvT2zKIn3X/8i4peEH+ll74fg38FnSbNd67IJKusm7Xi+fT8r87cm
     * NW1fiQG2SVufAQWbqz0lwcy2f8Lxb4bG+mRo64EtlOtCt/qMHt1i8b5QZ7dsvfPx
     * H2sMNgcWfzd8qVttevESRmCD1ycEvkvOl77DZypoEd+A5wwzZr8TDRRu838fYxAe
     * +o0bJW1sj6W3YQGx0qMmoRBxna3iw/nDmVG3KwcIzi7mULKn+gpFL6Lw8g==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1$0\"\x06\x03U\x04\x03\x13\x1bDigiCert Assured ID Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xad\x0e\x15\xce\xe4C\x80\\\xb1\x87\xf3\xb7`\xf9q\x12\xa5\xae\xdc&\x94\x88\xaa\xf4\xce\xf5 9(X`\x0c\xf8\x80\xda\xa9\x15\x952a<\xb5\xb1(\x84\x8a\x8a\xdc\x9f\n\x0c\x83\x17z\x8f\x90\xac\x8a\xe7yS\\1\x84*\xf6\x0f\x9826v\xcc\xde\xdd<\xa8\xa2\xefj\xfb!\xf2Ra\xdf\x9f \xd7\x1f\xe2\xb1\xd9\xfe\x18d\xd2\x12[_\xf9X\x185\xbcG\xcd\xa16\xf9k\x7f\xd4\xb08>\xc1\x1b\xc3\x8c3\xd9\xd8/\x18\xfe(\x0f\xb3\xa7\x83\xd6\xc3nD\xc0a5\x96\x16\xfeY\x9c\x8bvm\xd7\xf1\xa2K\r+\xff\x0br\xda\x9e`\xd0\x8e\x905\xc6xU\x87 \xa1\xcf\xe5m\n\xc8I|1\x983l\"\xe9\x87\xd02Z\xa2\xba\x13\x82\x11\xed9\x17\x9d\x99:r\xa1\xe6\xfa\xa4\xd9\xd5\x171u\xae\x85}\"\xae?\x01F\x86\xf6(y\xc8\xb1\xda\xe4W\x17\xc4~\x1c\x0e\xb0\xb4\x92\xa6V\xb3\xbd\xb2\x97\xed\xaa\xa7\xf0\xb7\xc5\xa8?\x95\x16\xd0\xff\xa1\x96\xeb\x08_\x18wO\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=emSign Root CA - G1 O=eMudhra Technologies Limited OU=emSign PKI
     * Subject: CN=emSign Root CA - G1 O=eMudhra Technologies Limited OU=emSign PKI
     * Label: "emSign Root CA - G1"
     * Serial: 235931866688319308814040
     * MD5 Fingerprint: 9c:42:84:57:dd:cb:0b:a7:2e:95:ad:b6:f3:da:bc:ac
     * SHA1 Fingerprint: 8a:c7:ad:8f:73:ac:4e:c1:b5:75:4d:a5:40:f4:fc:cf:7c:b5:8e:8c
     * SHA256 Fingerprint: 40:f6:af:03:46:a9:9a:a1:cd:1d:55:5a:4e:9c:ce:62:c7:f9:63:46:03:ee:40:66:15:83:3d:c8:c8:d0:03:67
     * -----BEGIN CERTIFICATE-----
     * MIIDlDCCAnygAwIBAgIKMfXkYgxsWO3W2DANBgkqhkiG9w0BAQsFADBnMQswCQYD
     * VQQGEwJJTjETMBEGA1UECxMKZW1TaWduIFBLSTElMCMGA1UEChMcZU11ZGhyYSBU
     * ZWNobm9sb2dpZXMgTGltaXRlZDEcMBoGA1UEAxMTZW1TaWduIFJvb3QgQ0EgLSBH
     * MTAeFw0xODAyMTgxODMwMDBaFw00MzAyMTgxODMwMDBaMGcxCzAJBgNVBAYTAklO
     * MRMwEQYDVQQLEwplbVNpZ24gUEtJMSUwIwYDVQQKExxlTXVkaHJhIFRlY2hub2xv
     * Z2llcyBMaW1pdGVkMRwwGgYDVQQDExNlbVNpZ24gUm9vdCBDQSAtIEcxMIIBIjAN
     * BgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAk0u76WaK7p1b1TST0Bsew+eeuGQz
     * f2N4aLTNLnF115sgxk0pvLZoYIr3IZpWNVrzdr3YzZr/k1ZLpVkGoZM0Kd0WNHVO
     * 8oG0x5ZOrRkVUkr+PHB1cM2vK6sVmjM8qrOLqs1D/fXqcP/tzxE7lM5OMhbTI0Aq
     * d7OvPAEsbO2ZLIvZTmmYsvePQbAyeGHWDV/D+qJAkh1cF+ZwPjXnorfCYuKrpDhM
     * tTk1b+oDafo6VGiFbdbyL0NVHpENDtjVaqSW0RM8LHhQ6DqS0hdW5TUaQBw+jSzt
     * Od9C4INBdN+jzcKGYEho42kLVACL5HZpIQ15TjQIXhTCzLG3rdd8cIrHhQIDAQAB
     * o0IwQDAdBgNVHQ4EFgQU++8Nhp6w492pufEhF38+/PB3KxowDgYDVR0PAQH/BAQD
     * AgEGMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEBAFn/8oz1h31x
     * PaOfG1vR2vjTnGs2vZupYeveFix0PZ7mddrXuqe8QhfnPZHr5X3dPpzxz5KsbEjM
     * wiI/aTvFthUvozXGaCocV685743QNcMYDHsAVhzNixl03r4PEuDQqqE/AjSxcM6d
     * GNYIAwlG7mDgfrbESQRRfXBgvKqy/3lyeqYdPV8q+Mri/Tm3R7nrft8EI6/6nAYH
     * 6ftjk4BAtcZsCjEozgyfz7MjNYBBjWzEN3uBL4ChQEKF6dk4jeihU80Bv2noWgby
     * RQuQ+q7hv53yrlc8pa6yVvSLZUDp/TGBLPQ5Cdjua6e0ph0VpZj3AYHYhX3zUVxx
     * iN66zB+Afko=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02IN1\x130\x11\x06\x03U\x04\x0b\x13\nemSign PKI1%0#\x06\x03U\x04\n\x13\x1ceMudhra Technologies Limited1\x1c0\x1a\x06\x03U\x04\x03\x13\x13emSign Root CA - G1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\x93K\xbb\xe9f\x8a\xee\x9d[\xd54\x93\xd0\x1b\x1e\xc3\xe7\x9e\xb8d3\x7fcxh\xb4\xcd.qu\xd7\x9b \xc6M)\xbc\xb6h`\x8a\xf7!\x9aV5Z\xf3v\xbd\xd8\xcd\x9a\xff\x93VK\xa5Y\x06\xa1\x934)\xdd\x164uN\xf2\x81\xb4\xc7\x96N\xad\x19\x15RJ\xfe<pup\xcd\xaf+\xab\x15\x9a3<\xaa\xb3\x8b\xaa\xcdC\xfd\xf5\xeap\xff\xed\xcf\x11;\x94\xceN2\x16\xd3#@*w\xb3\xaf<\x01,l\xed\x99,\x8b\xd9Ni\x98\xb2\xf7\x8fA\xb02xa\xd6\r_\xc3\xfa\xa2@\x92\x1d\\\x17\xe6p>5\xe7\xa2\xb7\xc2b\xe2\xab\xa48L\xb595o\xea\x03i\xfa:Th\x85m\xd6\xf2/CU\x1e\x91\r\x0e\xd8\xd5j\xa4\x96\xd1\x13<,xP\xe8:\x92\xd2\x17V\xe55\x1a@\x1c>\x8d,\xed9\xdfB\xe0\x83At\xdf\xa3\xcd\xc2\x86`Hh\xe3i\x0bT\x00\x8b\xe4vi!\ryN4\x08^\x14\xc2\xcc\xb1\xb7\xad\xd7|p\x8a\xc7\x85\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Secure Global CA O=SecureTrust Corporation
     * Subject: CN=Secure Global CA O=SecureTrust Corporation
     * Label: "Secure Global CA"
     * Serial: 9751836167731051554232119481456978597
     * MD5 Fingerprint: cf:f4:27:0d:d4:ed:dc:65:16:49:6d:3d:da:bf:6e:de
     * SHA1 Fingerprint: 3a:44:73:5a:e5:81:90:1f:24:86:61:46:1e:3b:9c:c4:5f:f5:3a:1b
     * SHA256 Fingerprint: 42:00:f5:04:3a:c8:59:0e:bb:52:7d:20:9e:d1:50:30:29:fb:cb:d4:1c:a1:b5:06:ec:27:f1:5a:de:7d:ac:69
     * -----BEGIN CERTIFICATE-----
     * MIIDvDCCAqSgAwIBAgIQB1YipOjUiolN9BPI8PjqpTANBgkqhkiG9w0BAQUFADBK
     * MQswCQYDVQQGEwJVUzEgMB4GA1UEChMXU2VjdXJlVHJ1c3QgQ29ycG9yYXRpb24x
     * GTAXBgNVBAMTEFNlY3VyZSBHbG9iYWwgQ0EwHhcNMDYxMTA3MTk0MjI4WhcNMjkx
     * MjMxMTk1MjA2WjBKMQswCQYDVQQGEwJVUzEgMB4GA1UEChMXU2VjdXJlVHJ1c3Qg
     * Q29ycG9yYXRpb24xGTAXBgNVBAMTEFNlY3VyZSBHbG9iYWwgQ0EwggEiMA0GCSqG
     * SIb3DQEBAQUAA4IBDwAwggEKAoIBAQCvNS7YrGxVaQZx5RNoJLNP2MwhR/jxYDiJ
     * iQPpvepeRlMJ3Fz1Wuj3RSoC6zFh1ykzTM7HfAo3fg+6MpjhHZevj8fcyTiW89sa
     * /FHtaMbQbqR8JNGuQsiWUGMu4P51/pinX0kuleM5M2SOHqRfkNJnPLLZ/kG5VacJ
     * jnIFHovdRIWCQtBJwB1g8NEXLJXr9qXBkqPFwqcIYA1gBBCWeZ4WNOaptvolRTnI
     * HmX5k/Wq8VLcmZg9pYYaDDUz+kulBAYVHDGA76oYa8J719rO+TMg1fW9ajMtgQT7
     * sFzUnKPiXB3jqUJ1XnvUd+85VLrJChgbEplJL4hL/VBi0XPnj3pDAgMBAAGjgZ0w
     * gZowEwYJKwYBBAGCNxQCBAYeBABDAEEwCwYDVR0PBAQDAgGGMA8GA1UdEwEB/wQF
     * MAMBAf8wHQYDVR0OBBYEFK9EBMJBfkiD2045AuzshHrmzsmkMDQGA1UdHwQtMCsw
     * KaAnoCWGI2h0dHA6Ly9jcmwuc2VjdXJldHJ1c3QuY29tL1NHQ0EuY3JsMBAGCSsG
     * AQQBgjcVAQQDAgEAMA0GCSqGSIb3DQEBBQUAA4IBAQBjGghAfaReUw132HquHw0L
     * URYD7xh8yOOvaliTFGCRsoTciE6+OYo68+aCiV0BN7OrJKQVDpI1WkpEXk5X+nXO
     * H0jOZvQ8QCaSmGwb7iRGDBezUqXbpZGRzzfTb+cnCDpOGR86p1hcF895P4vkp9Mm
     * I50mD1hp/Ed+stCNi5O/KU9DaXR2Z0vPB4zmAve14bRDtUstFJ/53CYNv6ZHdAbY
     * iNE6KTCEztI5gGIbqMdXSbxqVVFnFUq+NQfk1XWYN3kwFNspnWzFacxHVaIw98xc
     * f8LDmBxrThaA63p4ZUWiABqvDA1VZDRIuJK58bRQKfJPIx/abKwfROHdI3hRW8cW
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1 0\x1e\x06\x03U\x04\n\x13\x17SecureTrust Corporation1\x190\x17\x06\x03U\x04\x03\x13\x10Secure Global CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xaf5.\xd8\xaclUi\x06q\xe5\x13h$\xb3O\xd8\xcc!G\xf8\xf1`8\x89\x89\x03\xe9\xbd\xea^FS\t\xdc\\\xf5Z\xe8\xf7E*\x02\xeb1a\xd7)3L\xce\xc7|\n7~\x0f\xba2\x98\xe1\x1d\x97\xaf\x8f\xc7\xdc\xc98\x96\xf3\xdb\x1a\xfcQ\xedh\xc6\xd0n\xa4|$\xd1\xaeB\xc8\x96Pc.\xe0\xfeu\xfe\x98\xa7_I.\x95\xe393d\x8e\x1e\xa4_\x90\xd2g<\xb2\xd9\xfeA\xb9U\xa7\t\x8er\x05\x1e\x8b\xddD\x85\x82B\xd0I\xc0\x1d`\xf0\xd1\x17,\x95\xeb\xf6\xa5\xc1\x92\xa3\xc5\xc2\xa7\x08`\r`\x04\x10\x96y\x9e\x164\xe6\xa9\xb6\xfa%E9\xc8\x1ee\xf9\x93\xf5\xaa\xf1R\xdc\x99\x98=\xa5\x86\x1a\x0c53\xfaK\xa5\x04\x06\x15\x1c1\x80\xef\xaa\x18k\xc2{\xd7\xda\xce\xf93 \xd5\xf5\xbdj3-\x81\x04\xfb\xb0\\\xd4\x9c\xa3\xe2\\\x1d\xe3\xa9Bu^{\xd4w\xef9T\xba\xc9\n\x18\x1b\x12\x99I/\x88K\xfdPb\xd1s\xe7\x8fzC\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Global Root CA O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Global Root CA O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Global Root CA"
     * Serial: 10944719598952040374951832963794454346
     * MD5 Fingerprint: 79:e4:a9:84:0d:7d:3a:96:d7:c0:4f:e2:43:4c:89:2e
     * SHA1 Fingerprint: a8:98:5d:3a:65:e5:e5:c4:b2:d7:d6:6d:40:c6:dd:2f:b1:9c:54:36
     * SHA256 Fingerprint: 43:48:a0:e9:44:4c:78:cb:26:5e:05:8d:5e:89:44:b4:d8:4f:96:62:bd:26:db:25:7f:89:34:a4:43:c7:01:61
     * -----BEGIN CERTIFICATE-----
     * MIIDrzCCApegAwIBAgIQCDvgVpBCRrGhdWrJWZHHSjANBgkqhkiG9w0BAQUFADBh
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSAwHgYDVQQDExdEaWdpQ2VydCBHbG9iYWwgUm9vdCBD
     * QTAeFw0wNjExMTAwMDAwMDBaFw0zMTExMTAwMDAwMDBaMGExCzAJBgNVBAYTAlVT
     * MRUwEwYDVQQKEwxEaWdpQ2VydCBJbmMxGTAXBgNVBAsTEHd3dy5kaWdpY2VydC5j
     * b20xIDAeBgNVBAMTF0RpZ2lDZXJ0IEdsb2JhbCBSb290IENBMIIBIjANBgkqhkiG
     * 9w0BAQEFAAOCAQ8AMIIBCgKCAQEA4jvhEXLeqKTTo1eqUKKPC3eQyaKl7hLOllsB
     * CSDMAZOnTjC3U/dDxGkAV53ijSLdhwZAAIEJzs4bg7/fzTtxRuLWZscFs3YnFo97
     * nh6Vfe63SKMI2tavegw5BmV/Sl0fvBf4q77uKNd0f3p4mVmFaG5cIzJLv07A6Fpt
     * 43C/dxC//AH2hdmoRBBYMql1GNXRor5H4idq9Joz+EkIYIvUX7Q6hL+hqkpMfT7P
     * T19sdl6gSzeRntwi5m3OFBqOasv+zbMUZBfHWymeMr/y7vrTC0LUq7dBMtoM1O/4
     * gdW7jVg/tRvoSSiicNoxBN33shbyTApOB6jtSj1etX+jkMOvJwIDAQABo2MwYTAO
     * BgNVHQ8BAf8EBAMCAYYwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUA95QNVbR
     * TLtm8KPiGxvDl7I90VUwHwYDVR0jBBgwFoAUA95QNVbRTLtm8KPiGxvDl7I90VUw
     * DQYJKoZIhvcNAQEFBQADggEBAMucN6pIExIK+t1EnE9SsPTfrgT1eXkIoyQY/Esr
     * hMAtudXH/vTBH1jLuG2cenTnmCmrEbXjcKChzUyImZOMkXDiqw8cvpOp/2PV5Adg
     * 06O/nVsJ8dWO41P0jmP6P6fbtGbfYmbW0W5BjfIttep3Sp+dWOIrWcBAI+0tKIJF
     * PnlUkiaY4IBIqDfv8NZ5YBberOgOzW6sRBc4L0na4UU+Krk2U886UAb3LujEV0ls
     * YSEY1QSteDwsOoBrp+uvFRTp2InBuThs4pFsiv9kuXclVzDAGySj4dzp30d8tbQk
     * CAUw7C29C79Fv1C5qfPrmAESrciIxpg0X40KPMbp1ZWVbd4=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1 0\x1e\x06\x03U\x04\x03\x13\x17DigiCert Global Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xe2;\xe1\x11r\xde\xa8\xa4\xd3\xa3W\xaaP\xa2\x8f\x0bw\x90\xc9\xa2\xa5\xee\x12\xce\x96[\x01\t \xcc\x01\x93\xa7N0\xb7S\xf7C\xc4i\x00W\x9d\xe2\x8d\"\xdd\x87\x06@\x00\x81\t\xce\xce\x1b\x83\xbf\xdf\xcd;qF\xe2\xd6f\xc7\x05\xb3v\'\x16\x8f{\x9e\x1e\x95}\xee\xb7H\xa3\x08\xda\xd6\xafz\x0c9\x06e\x7fJ]\x1f\xbc\x17\xf8\xab\xbe\xee(\xd7t\x7fzx\x99Y\x85hn\\#2K\xbfN\xc0\xe8Zm\xe3p\xbfw\x10\xbf\xfc\x01\xf6\x85\xd9\xa8D\x10X2\xa9u\x18\xd5\xd1\xa2\xbeG\xe2\'j\xf4\x9a3\xf8I\x08`\x8b\xd4_\xb4:\x84\xbf\xa1\xaaJL}>\xcfO_lv^\xa0K7\x91\x9e\xdc\"\xe6m\xce\x14\x1a\x8ej\xcb\xfe\xcd\xb3\x14d\x17\xc7[)\x9e2\xbf\xf2\xee\xfa\xd3\x0bB\xd4\xab\xb7A2\xda\x0c\xd4\xef\xf8\x81\xd5\xbb\x8dX?\xb5\x1b\xe8I(\xa2p\xda1\x04\xdd\xf7\xb2\x16\xf2L\nN\x07\xa8\xedJ=^\xb5\x7f\xa3\x90\xc3\xaf\'\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Entrust Root Certification Authority - G2 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2009 Entrust, Inc. - for authorized use only
     * Subject: CN=Entrust Root Certification Authority - G2 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2009 Entrust, Inc. - for authorized use only
     * Label: "Entrust Root Certification Authority - G2"
     * Serial: 1246989352
     * MD5 Fingerprint: 4b:e2:c9:91:96:65:0c:f4:0e:5a:93:92:a0:0a:fe:b2
     * SHA1 Fingerprint: 8c:f4:27:fd:79:0c:3a:d1:66:06:8d:e8:1e:57:ef:bb:93:22:72:d4
     * SHA256 Fingerprint: 43:df:57:74:b0:3e:7f:ef:5f:e4:0d:93:1a:7b:ed:f1:bb:2e:6b:42:73:8c:4e:6d:38:41:10:3d:3a:a7:f3:39
     * -----BEGIN CERTIFICATE-----
     * MIIEPjCCAyagAwIBAgIESlOMKDANBgkqhkiG9w0BAQsFADCBvjELMAkGA1UEBhMC
     * VVMxFjAUBgNVBAoTDUVudHJ1c3QsIEluYy4xKDAmBgNVBAsTH1NlZSB3d3cuZW50
     * cnVzdC5uZXQvbGVnYWwtdGVybXMxOTA3BgNVBAsTMChjKSAyMDA5IEVudHJ1c3Qs
     * IEluYy4gLSBmb3IgYXV0aG9yaXplZCB1c2Ugb25seTEyMDAGA1UEAxMpRW50cnVz
     * dCBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5IC0gRzIwHhcNMDkwNzA3MTcy
     * NTU0WhcNMzAxMjA3MTc1NTU0WjCBvjELMAkGA1UEBhMCVVMxFjAUBgNVBAoTDUVu
     * dHJ1c3QsIEluYy4xKDAmBgNVBAsTH1NlZSB3d3cuZW50cnVzdC5uZXQvbGVnYWwt
     * dGVybXMxOTA3BgNVBAsTMChjKSAyMDA5IEVudHJ1c3QsIEluYy4gLSBmb3IgYXV0
     * aG9yaXplZCB1c2Ugb25seTEyMDAGA1UEAxMpRW50cnVzdCBSb290IENlcnRpZmlj
     * YXRpb24gQXV0aG9yaXR5IC0gRzIwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEK
     * AoIBAQC6hLZy254Ma+KZ6TABp3bqMriVQRrJ2mFOWHLP/vaCeb9zYQYKpSfYs1/T
     * RU4cctZOMvJyig/3gxnQaoCAAEUesMfnmr8SVycco2gvCoe9amsOXmXzHHfV1IWN
     * cCG0szLni6LVhjkCsbjSR87kyUnEO6fe+1R9V77w6G7CebI6C1XiUJgWMhNcL3hW
     * wcKUs/Ja5CeanyTXxuzQmyWC48zCxEXFjJd6BmsqEZ+pCm5IO2/b1BEZQvePB7/1
     * U1+cPvQXLOZprE4yTGJ36rfo5bs0vBmLrpxR57d+tVOxMyLlbc9wPBr64ptntoP0
     * jaWvYkxN4FisZDQSA/i2jZRjJKRxAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAP
     * BgNVHRMBAf8EBTADAQH/MB0GA1UdDgQWBBRqciZ60B7vfec7aVHUbI2fkBJmqzAN
     * BgkqhkiG9w0BAQsFAAOCAQEAeZ8dlsa2eT8ijYfThwMEYGprmi5ZiXMRrEPR9RP/
     * jTkrwPK9T3CMqS/qF8QLVJ7UG5aYMzyorWKiAHarWWluBh1+xLlEjZivEtRh2woZ
     * Rkfz6/djwUAFQKXSt/S1mja/qYh2iARVBCuch38aNzx+LaUa2NSJXsq9rD1s2G2v
     * 1fN2D807iDginWyTmsQ9v4IbZT+mD12q/OWyFcq1rca8PdCE6OoGcrBNOTJ4vz4R
     * nAuknZoh8/CbCzB428Hch0P+vGOaysXCHMnHjf87ElgI5rY97HosTvuDls4MPGmH
     * VHOkc8KT/1EQrBVUAdj8BbGJoX90g5pJ19xOe4pIb4tF9g==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x160\x14\x06\x03U\x04\n\x13\rEntrust, Inc.1(0&\x06\x03U\x04\x0b\x13\x1fSee www.entrust.net/legal-terms1907\x06\x03U\x04\x0b\x130(c) 2009 Entrust, Inc. - for authorized use only1200\x06\x03U\x04\x03\x13)Entrust Root Certification Authority - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xba\x84\xb6r\xdb\x9e\x0ck\xe2\x99\xe90\x01\xa7v\xea2\xb8\x95A\x1a\xc9\xdaaNXr\xcf\xfe\xf6\x82y\xbfsa\x06\n\xa5\'\xd8\xb3_\xd3EN\x1cr\xd6N2\xf2r\x8a\x0f\xf7\x83\x19\xd0j\x80\x80\x00E\x1e\xb0\xc7\xe7\x9a\xbf\x12W\'\x1c\xa3h/\n\x87\xbdjk\x0e^e\xf3\x1cw\xd5\xd4\x85\x8dp!\xb4\xb32\xe7\x8b\xa2\xd5\x869\x02\xb1\xb8\xd2G\xce\xe4\xc9I\xc4;\xa7\xde\xfbT}W\xbe\xf0\xe8n\xc2y\xb2:\x0bU\xe2P\x98\x162\x13\\/xV\xc1\xc2\x94\xb3\xf2Z\xe4\'\x9a\x9f$\xd7\xc6\xec\xd0\x9b%\x82\xe3\xcc\xc2\xc4E\xc5\x8c\x97z\x06k*\x11\x9f\xa9\nnH;o\xdb\xd4\x11\x19B\xf7\x8f\x07\xbf\xf5S_\x9c>\xf4\x17,\xe6i\xacN2Lbw\xea\xb7\xe8\xe5\xbb4\xbc\x19\x8b\xae\x9cQ\xe7\xb7~\xb5S\xb13\"\xe5m\xcfp<\x1a\xfa\xe2\x9bg\xb6\x83\xf4\x8d\xa5\xafbLM\xe0X\xacd4\x12\x03\xf8\xb6\x8d\x94c$\xa4q\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Hellenic Academic and Research Institutions ECC RootCA 2015 O=Hellenic Academic and Research Institutions Cert. Authority
     * Subject: CN=Hellenic Academic and Research Institutions ECC RootCA 2015 O=Hellenic Academic and Research Institutions Cert. Authority
     * Label: "Hellenic Academic and Research Institutions ECC RootCA 2015"
     * Serial: 0
     * MD5 Fingerprint: 81:e5:b4:17:eb:c2:f5:e1:4b:0d:41:7b:49:92:fe:ef
     * SHA1 Fingerprint: 9f:f1:71:8d:92:d5:9a:f3:7d:74:97:b4:bc:6f:84:68:0b:ba:b6:66
     * SHA256 Fingerprint: 44:b5:45:aa:8a:25:e6:5a:73:ca:15:dc:27:fc:36:d2:4c:1c:b9:95:3a:06:65:39:b1:15:82:dc:48:7b:48:33
     * -----BEGIN CERTIFICATE-----
     * MIICwzCCAkqgAwIBAgIBADAKBggqhkjOPQQDAjCBqjELMAkGA1UEBhMCR1IxDzAN
     * BgNVBAcTBkF0aGVuczFEMEIGA1UEChM7SGVsbGVuaWMgQWNhZGVtaWMgYW5kIFJl
     * c2VhcmNoIEluc3RpdHV0aW9ucyBDZXJ0LiBBdXRob3JpdHkxRDBCBgNVBAMTO0hl
     * bGxlbmljIEFjYWRlbWljIGFuZCBSZXNlYXJjaCBJbnN0aXR1dGlvbnMgRUNDIFJv
     * b3RDQSAyMDE1MB4XDTE1MDcwNzEwMzcxMloXDTQwMDYzMDEwMzcxMlowgaoxCzAJ
     * BgNVBAYTAkdSMQ8wDQYDVQQHEwZBdGhlbnMxRDBCBgNVBAoTO0hlbGxlbmljIEFj
     * YWRlbWljIGFuZCBSZXNlYXJjaCBJbnN0aXR1dGlvbnMgQ2VydC4gQXV0aG9yaXR5
     * MUQwQgYDVQQDEztIZWxsZW5pYyBBY2FkZW1pYyBhbmQgUmVzZWFyY2ggSW5zdGl0
     * dXRpb25zIEVDQyBSb290Q0EgMjAxNTB2MBAGByqGSM49AgEGBSuBBAAiA2IABJKg
     * QehLgoRc4vgxEZmGZE4JJS+dQS8KrjVPdJWyUWRrjWvmP3CV8AVER6ZyOFB2lQJa
     * jq4onvktTpnvLEhvTCUp6NFxW98dwXU3tNf6e3pCnGoKVlp8aQuqgAkkbH7BRqNC
     * MEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFLQi
     * C4KZJAEOnLvkDv2/+5cgk5kqMAoGCCqGSM49BAMCA2cAMGQCMGfOFmI4oqxiRaep
     * lSTAGiecMjvAwNW6qef4BENThe5SId6d9SWDPp5YSy/XZxMOIQIwBeF1Ad5o7Sof
     * TUwJCA3sS61kFyjndc5FZXIhF8siQQ6ME5g4mlRtm8rifOoCWCKR
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GR1\x0f0\r\x06\x03U\x04\x07\x13\x06Athens1D0B\x06\x03U\x04\n\x13;Hellenic Academic and Research Institutions Cert. Authority1D0B\x06\x03U\x04\x03\x13;Hellenic Academic and Research Institutions ECC RootCA 2015",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x92\xa0A\xe8K\x82\x84\\\xe2\xf81\x11\x99\x86dN\t%/\x9dA/\n\xae5Ot\x95\xb2Qdk\x8dk\xe6?p\x95\xf0\x05DG\xa6r8Pv\x95\x02Z\x8e\xae(\x9e\xf9-N\x99\xef,HoL%)\xe8\xd1q[\xdf\x1d\xc1u7\xb4\xd7\xfa{zB\x9cj\nVZ|i\x0b\xaa\x80\t$l~\xc1F",
        name_constraints: None
    },

    /*
     * Issuer: CN=Go Daddy Root Certificate Authority - G2 O=GoDaddy.com, Inc.
     * Subject: CN=Go Daddy Root Certificate Authority - G2 O=GoDaddy.com, Inc.
     * Label: "Go Daddy Root Certificate Authority - G2"
     * Serial: 0
     * MD5 Fingerprint: 80:3a:bc:22:c1:e6:fb:8d:9b:3b:27:4a:32:1b:9a:01
     * SHA1 Fingerprint: 47:be:ab:c9:22:ea:e8:0e:78:78:34:62:a7:9f:45:c2:54:fd:e6:8b
     * SHA256 Fingerprint: 45:14:0b:32:47:eb:9c:c8:c5:b4:f0:d7:b5:30:91:f7:32:92:08:9e:6e:5a:63:e2:74:9d:d3:ac:a9:19:8e:da
     * -----BEGIN CERTIFICATE-----
     * MIIDxTCCAq2gAwIBAgIBADANBgkqhkiG9w0BAQsFADCBgzELMAkGA1UEBhMCVVMx
     * EDAOBgNVBAgTB0FyaXpvbmExEzARBgNVBAcTClNjb3R0c2RhbGUxGjAYBgNVBAoT
     * EUdvRGFkZHkuY29tLCBJbmMuMTEwLwYDVQQDEyhHbyBEYWRkeSBSb290IENlcnRp
     * ZmljYXRlIEF1dGhvcml0eSAtIEcyMB4XDTA5MDkwMTAwMDAwMFoXDTM3MTIzMTIz
     * NTk1OVowgYMxCzAJBgNVBAYTAlVTMRAwDgYDVQQIEwdBcml6b25hMRMwEQYDVQQH
     * EwpTY290dHNkYWxlMRowGAYDVQQKExFHb0RhZGR5LmNvbSwgSW5jLjExMC8GA1UE
     * AxMoR28gRGFkZHkgUm9vdCBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkgLSBHMjCCASIw
     * DQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAL9xYgjx+lk09xvJGKP3gElY6SKD
     * E6bFIEMBO4Tx5oVJnyfq9oQbTqC023CYxzIBsQU+B07u9PpPL1kwIuerGVZr4oAH
     * /PMWdYA5UXvl+TW2dE6pjYIT5LY/qQOD+qK+ihVqf94Lw7YZFAXK6sOoBJQ7Rnwy
     * DfMAZiLIjWltNowRGLfTshxgtDj6AozO091GB94KPutdfMh8+7ArU6SSYmlRJQVh
     * GkSBjCypQ5Yj36w6gZoOKcUcqeldHraenjAKOc7xiID7S13MMuyFYkMlNAJWJwGR
     * tDtwKj9useiciAF9n9T521NtYJ2/LOdYq7hfRvzOxBsDPAnrSTFcaUaz4EcCAwEA
     * AaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYE
     * FDqahQcQZyi27/a9BUFuIMGU2g/eMA0GCSqGSIb3DQEBCwUAA4IBAQCZ21151fmX
     * WWcDYfF+OwYxdS2hII5PZYe096acvNjpL9DbWu7PdIxztDhC2gV7+AJ1uP2lsdeu
     * 9tfeE8tTEH6KRtGX+rcuKxGrkLAngPnon1rpN5+r5N9ss4UXnT3ZJE95kTXWXwTr
     * gIOrmgIttRD02JDHBHNA7XIloKmf7J6raBKZV8aPEjoJpL1E/QYVN8Gb5DKj7Tjo
     * 2GTzLH4U/ALqn83/B2gX2yKQOC16jdFU8WnjXzPKej17CuPKf1855eJ1usV2GDPO
     * LPAvTK33sefOT6jEm0pUBsV/fdUID+Ic/n4XuKxe9tQWskMJDE32p2u0mYRlynqI
     * 4uJEvlz36hz1
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x100\x0e\x06\x03U\x04\x08\x13\x07Arizona1\x130\x11\x06\x03U\x04\x07\x13\nScottsdale1\x1a0\x18\x06\x03U\x04\n\x13\x11GoDaddy.com, Inc.110/\x06\x03U\x04\x03\x13(Go Daddy Root Certificate Authority - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbfqb\x08\xf1\xfaY4\xf7\x1b\xc9\x18\xa3\xf7\x80IX\xe9\"\x83\x13\xa6\xc5 C\x01;\x84\xf1\xe6\x85I\x9f\'\xea\xf6\x84\x1bN\xa0\xb4\xdbp\x98\xc72\x01\xb1\x05>\x07N\xee\xf4\xfaO/Y0\"\xe7\xab\x19Vk\xe2\x80\x07\xfc\xf3\x16u\x809Q{\xe5\xf95\xb6tN\xa9\x8d\x82\x13\xe4\xb6?\xa9\x03\x83\xfa\xa2\xbe\x8a\x15j\x7f\xde\x0b\xc3\xb6\x19\x14\x05\xca\xea\xc3\xa8\x04\x94;F|2\r\xf3\x00f\"\xc8\x8dim6\x8c\x11\x18\xb7\xd3\xb2\x1c`\xb48\xfa\x02\x8c\xce\xd3\xddF\x07\xde\n>\xeb]|\xc8|\xfb\xb0+S\xa4\x92biQ%\x05a\x1aD\x81\x8c,\xa9C\x96#\xdf\xac:\x81\x9a\x0e)\xc5\x1c\xa9\xe9]\x1e\xb6\x9e\x9e0\n9\xce\xf1\x88\x80\xfbK]\xcc2\xec\x85bC%4\x02V\'\x01\x91\xb4;p*?n\xb1\xe8\x9c\x88\x01}\x9f\xd4\xf9\xdbSm`\x9d\xbf,\xe7X\xab\xb8_F\xfc\xce\xc4\x1b\x03<\t\xebI1\\iF\xb3\xe0G\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TUBITAK Kamu SM SSL Kok Sertifikasi - Surum 1 O=Turkiye Bilimsel ve Teknolojik Arastirma Kurumu - TUBITAK OU=Kamu Sertifikasyon Merkezi - Kamu SM
     * Subject: CN=TUBITAK Kamu SM SSL Kok Sertifikasi - Surum 1 O=Turkiye Bilimsel ve Teknolojik Arastirma Kurumu - TUBITAK OU=Kamu Sertifikasyon Merkezi - Kamu SM
     * Label: "TUBITAK Kamu SM SSL Kok Sertifikasi - Surum 1"
     * Serial: 1
     * MD5 Fingerprint: dc:00:81:dc:69:2f:3e:2f:b0:3b:f6:3d:5a:91:8e:49
     * SHA1 Fingerprint: 31:43:64:9b:ec:ce:27:ec:ed:3a:3f:0b:8f:0d:e4:e8:91:dd:ee:ca
     * SHA256 Fingerprint: 46:ed:c3:68:90:46:d5:3a:45:3f:b3:10:4a:b8:0d:ca:ec:65:8b:26:60:ea:16:29:dd:7e:86:79:90:64:87:16
     * -----BEGIN CERTIFICATE-----
     * MIIEYzCCA0ugAwIBAgIBATANBgkqhkiG9w0BAQsFADCB0jELMAkGA1UEBhMCVFIx
     * GDAWBgNVBAcTD0dlYnplIC0gS29jYWVsaTFCMEAGA1UEChM5VHVya2l5ZSBCaWxp
     * bXNlbCB2ZSBUZWtub2xvamlrIEFyYXN0aXJtYSBLdXJ1bXUgLSBUVUJJVEFLMS0w
     * KwYDVQQLEyRLYW11IFNlcnRpZmlrYXN5b24gTWVya2V6aSAtIEthbXUgU00xNjA0
     * BgNVBAMTLVRVQklUQUsgS2FtdSBTTSBTU0wgS29rIFNlcnRpZmlrYXNpIC0gU3Vy
     * dW0gMTAeFw0xMzExMjUwODI1NTVaFw00MzEwMjUwODI1NTVaMIHSMQswCQYDVQQG
     * EwJUUjEYMBYGA1UEBxMPR2ViemUgLSBLb2NhZWxpMUIwQAYDVQQKEzlUdXJraXll
     * IEJpbGltc2VsIHZlIFRla25vbG9qaWsgQXJhc3Rpcm1hIEt1cnVtdSAtIFRVQklU
     * QUsxLTArBgNVBAsTJEthbXUgU2VydGlmaWthc3lvbiBNZXJrZXppIC0gS2FtdSBT
     * TTE2MDQGA1UEAxMtVFVCSVRBSyBLYW11IFNNIFNTTCBLb2sgU2VydGlmaWthc2kg
     * LSBTdXJ1bSAxMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAr3UwM6q7
     * a9OZLBI3hNmNe5eA027n/5tQlT6QlVZC1xl8JoSNkvoBHToP4mQ4t4y86Ij5iySr
     * LqP1N+RAjhgleYN1Hzv/bKjFxlb4tO2KRKOrbEz8HdDc72i9z+SqzvBV96I01INr
     * N3wcwv61A+xXzry0tcXtAA9TNypN9E8Mg/uGz8v+jE69h/mniyFXnHrfA2eJLJ2X
     * YacQuFWQfw4tJzh03+f92k4S400VIgLI4OD8D62K18lUUMw7D8oWgITQUVbDjlZ/
     * iSIzL+aFCr2lqBs23tPcLG07xxO9WSMs5uWk99gL7eqQQESolbuT1dCANLZGeA4f
     * AJNG4e7p+exPFwIDAQABo0IwQDAdBgNVHQ4EFgQUZT/HiobGPN08VFw1+DrtUgxH
     * V8gwDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQEL
     * BQADggEBACo/4fEyjq7hmFxLXs9rHmoJ0iKpEsdeV31zVmSAhHqT5Am5EM2fKifh
     * AHe+SMg1qIGf5LgsyX8OsNJLN13qudULXjS99HMpw+0mFZx+CFOKWI3QSyjfwbPf
     * IPP54+M638yclNhOT8NrF7f3cuitZjO1JVOr4PhMqZ398g26rrnZqsZr+ZO7rqu4
     * lzwDGrpDxpa5RXI4s6ehlj2Re37AIVNMh+3yC1SVUZPVIqUNivGTDj5UDrDYyU7c
     * 8jEyVupk+eq1nRZmQnLzf9OxMUP8pI4X8W0jq5Rm+K37DwhuJi1/FwcJsoz7UMCf
     * lo3Ptv0AnVoUmr8CRPXBwp8iXqIPoeM=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02TR1\x180\x16\x06\x03U\x04\x07\x13\x0fGebze - Kocaeli1B0@\x06\x03U\x04\n\x139Turkiye Bilimsel ve Teknolojik Arastirma Kurumu - TUBITAK1-0+\x06\x03U\x04\x0b\x13$Kamu Sertifikasyon Merkezi - Kamu SM1604\x06\x03U\x04\x03\x13-TUBITAK Kamu SM SSL Kok Sertifikasi - Surum 1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xafu03\xaa\xbbk\xd3\x99,\x127\x84\xd9\x8d{\x97\x80\xd3n\xe7\xff\x9bP\x95>\x90\x95VB\xd7\x19|&\x84\x8d\x92\xfa\x01\x1d:\x0f\xe2d8\xb7\x8c\xbc\xe8\x88\xf9\x8b$\xab.\xa3\xf57\xe4@\x8e\x18%y\x83u\x1f;\xffl\xa8\xc5\xc6V\xf8\xb4\xed\x8aD\xa3\xablL\xfc\x1d\xd0\xdc\xefh\xbd\xcf\xe4\xaa\xce\xf0U\xf7\xa24\xd4\x83k7|\x1c\xc2\xfe\xb5\x03\xecW\xce\xbc\xb4\xb5\xc5\xed\x00\x0fS7*M\xf4O\x0c\x83\xfb\x86\xcf\xcb\xfe\x8cN\xbd\x87\xf9\xa7\x8b!W\x9cz\xdf\x03g\x89,\x9d\x97a\xa7\x10\xb8U\x90\x7f\x0e-\'8t\xdf\xe7\xfd\xdaN\x12\xe3M\x15\"\x02\xc8\xe0\xe0\xfc\x0f\xad\x8a\xd7\xc9TP\xcc;\x0f\xca\x16\x80\x84\xd0QV\xc3\x8eV\x7f\x89\"3/\xe6\x85\n\xbd\xa5\xa8\x1b6\xde\xd3\xdc,m;\xc7\x13\xbdY#,\xe6\xe5\xa4\xf7\xd8\x0b\xed\xea\x90@D\xa8\x95\xbb\x93\xd5\xd0\x804\xb6Fx\x0e\x1f\x00\x93F\xe1\xee\xe9\xf9\xecO\x17\x02\x03\x01\x00\x01",
        name_constraints: Some(b"\xa0g0e\xa0c0\t\x82\x07.gov.tr0\t\x82\x07.k12.tr0\t\x82\x07.pol.tr0\t\x82\x07.mil.tr0\t\x82\x07.tsk.tr0\t\x82\x07.kep.tr0\t\x82\x07.bel.tr0\t\x82\x07.edu.tr0\t\x82\x07.org.tr")
    },

    /*
     * Issuer: CN=D-TRUST Root Class 3 CA 2 2009 O=D-Trust GmbH
     * Subject: CN=D-TRUST Root Class 3 CA 2 2009 O=D-Trust GmbH
     * Label: "D-TRUST Root Class 3 CA 2 2009"
     * Serial: 623603
     * MD5 Fingerprint: cd:e0:25:69:8d:47:ac:9c:89:35:90:f7:fd:51:3d:2f
     * SHA1 Fingerprint: 58:e8:ab:b0:36:15:33:fb:80:f7:9b:1b:6d:29:d3:ff:8d:5f:00:f0
     * SHA256 Fingerprint: 49:e7:a4:42:ac:f0:ea:62:87:05:00:54:b5:25:64:b6:50:e4:f4:9e:42:e3:48:d6:aa:38:e0:39:e9:57:b1:c1
     * -----BEGIN CERTIFICATE-----
     * MIIEMzCCAxugAwIBAgIDCYPzMA0GCSqGSIb3DQEBCwUAME0xCzAJBgNVBAYTAkRF
     * MRUwEwYDVQQKDAxELVRydXN0IEdtYkgxJzAlBgNVBAMMHkQtVFJVU1QgUm9vdCBD
     * bGFzcyAzIENBIDIgMjAwOTAeFw0wOTExMDUwODM1NThaFw0yOTExMDUwODM1NTha
     * ME0xCzAJBgNVBAYTAkRFMRUwEwYDVQQKDAxELVRydXN0IEdtYkgxJzAlBgNVBAMM
     * HkQtVFJVU1QgUm9vdCBDbGFzcyAzIENBIDIgMjAwOTCCASIwDQYJKoZIhvcNAQEB
     * BQADggEPADCCAQoCggEBANOySs96R+91myP6Oi/WUEWJNTrGa9v+2wBoqOADER03
     * UAifTUpolDWzU9GUY6cgVq/eUXjsKj3zSEhQPgrfRlWLJ23DEE0NkVJD2IfgXU42
     * tSHKXzlABF9bfsyjxiupQB7ZNoTWSPOSHjRGICTBpFGOShrvUD9pXRl/RcPHAY9R
     * ySPocq60vFYJfxLLHLGvKZAKyVXMD9O0Gu1HNVpK7ZxzBCHQqr0ME7UAyiZsxGsM
     * lFqVlNpQmvH/pStmMaTJOKDfHR+4CS7zp+hnUquVH+BGPtikw8paxTGA6Eian5Rp
     * /hnd2HN8gcqW3o7tszIFZYQ05ub9VxC1X3a/L7AQDcUCAwEAAaOCARowggEWMA8G
     * A1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFP3aFMSfMN4hvR5COfyrYyNJ4PGEMA4G
     * A1UdDwEB/wQEAwIBBjCB0wYDVR0fBIHLMIHIMIGAoH6gfIZ6bGRhcDovL2RpcmVj
     * dG9yeS5kLXRydXN0Lm5ldC9DTj1ELVRSVVNUJTIwUm9vdCUyMENsYXNzJTIwMyUy
     * MENBJTIwMiUyMDIwMDksTz1ELVRydXN0JTIwR21iSCxDPURFP2NlcnRpZmljYXRl
     * cmV2b2NhdGlvbmxpc3QwQ6BBoD+GPWh0dHA6Ly93d3cuZC10cnVzdC5uZXQvY3Js
     * L2QtdHJ1c3Rfcm9vdF9jbGFzc18zX2NhXzJfMjAwOS5jcmwwDQYJKoZIhvcNAQEL
     * BQADggEBAH+X2zDI36ScfSF6gHDOFBJpiBSVYEQBrLLpME+bUMJm2H6NMLVwMeni
     * acfzcNsgFYbQDfC+rAF1hM5+n02/t2A7nPPKHeJeaNijnZflQGDSNiH+0LS4F9p0
     * o3/U37CYAqxva2ssJSRyoWXuJVrl5jLn8t+rSfrzkGkj2wTZ51xY/GXUl77M/C4K
     * zCUqNQT4YJEVdT1B/yMfGchs64JTBKbkTCJNjYy6zltz7GRUUG3RnFX7acM2w4y8
     * PIWmawomDeCTmGCufsYkl4phX5GOZpIJhzbNi5stPvZR1FDUWSi9g/LMKHtThm3Y
     * Johw1+qRzT65ysCQblrGXnRl11z+o+I=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02DE1\x150\x13\x06\x03U\x04\n\x0c\x0cD-Trust GmbH1\'0%\x06\x03U\x04\x03\x0c\x1eD-TRUST Root Class 3 CA 2 2009",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd3\xb2J\xcfzG\xefu\x9b#\xfa:/\xd6PE\x895:\xc6k\xdb\xfe\xdb\x00h\xa8\xe0\x03\x11\x1d7P\x08\x9fMJh\x945\xb3S\xd1\x94c\xa7 V\xaf\xdeQx\xec*=\xf3HHP>\n\xdfFU\x8b\'m\xc3\x10M\r\x91RC\xd8\x87\xe0]N6\xb5!\xca_9@\x04_[~\xcc\xa3\xc6+\xa9@\x1e\xd96\x84\xd6H\xf3\x92\x1e4F $\xc1\xa4Q\x8eJ\x1a\xefP?i]\x19\x7fE\xc3\xc7\x01\x8fQ\xc9#\xe8r\xae\xb4\xbcV\t\x7f\x12\xcb\x1c\xb1\xaf)\x90\n\xc9U\xcc\x0f\xd3\xb4\x1a\xedG5ZJ\xed\x9cs\x04!\xd0\xaa\xbd\x0c\x13\xb5\x00\xca&l\xc4k\x0c\x94Z\x95\x94\xdaP\x9a\xf1\xff\xa5+f1\xa4\xc98\xa0\xdf\x1d\x1f\xb8\t.\xf3\xa7\xe8gR\xab\x95\x1f\xe0F>\xd8\xa4\xc3\xcaZ\xc51\x80\xe8H\x9a\x9f\x94i\xfe\x19\xdd\xd8s|\x81\xca\x96\xde\x8e\xed\xb32\x05e\x844\xe6\xe6\xfdW\x10\xb5_v\xbf/\xb0\x10\r\xc5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Staat der Nederlanden EV Root CA O=Staat der Nederlanden
     * Subject: CN=Staat der Nederlanden EV Root CA O=Staat der Nederlanden
     * Label: "Staat der Nederlanden EV Root CA"
     * Serial: 10000013
     * MD5 Fingerprint: fc:06:af:7b:e8:1a:f1:9a:b4:e8:d2:70:1f:c0:f5:ba
     * SHA1 Fingerprint: 76:e2:7e:c1:4f:db:82:c1:c0:a6:75:b5:05:be:3d:29:b4:ed:db:bb
     * SHA256 Fingerprint: 4d:24:91:41:4c:fe:95:67:46:ec:4c:ef:a6:cf:6f:72:e2:8a:13:29:43:2f:9d:8a:90:7a:c4:cb:5d:ad:c1:5a
     * -----BEGIN CERTIFICATE-----
     * MIIFcDCCA1igAwIBAgIEAJiWjTANBgkqhkiG9w0BAQsFADBYMQswCQYDVQQGEwJO
     * TDEeMBwGA1UECgwVU3RhYXQgZGVyIE5lZGVybGFuZGVuMSkwJwYDVQQDDCBTdGFh
     * dCBkZXIgTmVkZXJsYW5kZW4gRVYgUm9vdCBDQTAeFw0xMDEyMDgxMTE5MjlaFw0y
     * MjEyMDgxMTEwMjhaMFgxCzAJBgNVBAYTAk5MMR4wHAYDVQQKDBVTdGFhdCBkZXIg
     * TmVkZXJsYW5kZW4xKTAnBgNVBAMMIFN0YWF0IGRlciBOZWRlcmxhbmRlbiBFViBS
     * b290IENBMIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA48d+ifkkSzrS
     * M4M1LGns3Amk41GoJSt5uAg94JG6hIXGhaTK5skuU6TJJB79VWZxXSzFYGgEt9nC
     * UiY4iKTWO0Cmws0/zZiTs1QUWJZV1VD+hq2kY39ch/aO5ieSZxeSAgMs3NZmdO3d
     * Z//BYY1jTw+bbRcwJu+r0h8QoPnFfxZpgQNH7R5ojXKhTbImxrpsX23Wr9GxE46p
     * rfNeaXUmGD5BKyF/7otdBwadQ8QpCiv8Kj6GyzyDOvnJDdrFmeK8eEEzduG/L13l
     * pJhQDBXd4Pqcfzho0LKmeqfRMb1+ilgnQ7O6M5HTp5gVXJrm0w912fxBmJc+qiXb
     * j5IusHsMX/FjqTf5m3VpTCgmJdrV8hJwRVXj33NeN/UhbJCONVrJ0yPr08C+eKxC
     * KFhmpUZtcALXEPlLVPxdhkqHz3/KRawRWrUgUY0viEeXOcDPusBCAUCZSCELa6fS
     * /ZbV0b5GnUngC6agIk440ME8MLxwjyx1zNDFjFE7PZQIZCZhfbnDZY8UnCHQqv0X
     * cgOPvZuM5l5Tnrmd74K74bzickFbIZTTRTeU0d8JOV3nI6qaHcptqAqGhYqCvkIH
     * 1vI4gnPah1vlPNOePqc7nvQDs/nxfRN0Av+7oeX6AHkcpmZBiFxgV6YuCcS6/ZrP
     * px9Aw7vMWgpVSzs4dlG4Y4uElBbmVvMCAwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB
     * /zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFP6rAJCYniT8qcwaivsnuL8wbqg7
     * MA0GCSqGSIb3DQEBCwUAA4ICAQDPdyxuVr5Os7aEAJSrR8kN0nbHhp8dB9O2tLsI
     * eK9p0gtJ3jPFrK3CiAJ9Brc1AsFgyb/E6JTe1NOpEyVa/m6irn0F3H3zbPB+po3u
     * 2dfOWBfoqSmuc0iH55vKbimhZF8ZE/euBhD/UcabTVUlT5OZEAFTdfETzsemQUHS
     * v4ilf0X8rLiltTMMgsT7B/Zq5SWEXwbKwYY5EdtYzXc7LMJMD16a4/CrPmEbUCTC
     * wPTxGfARKbalGAKb12NMcIxHowNDXLldRqANb/9Zjr7dn3LDWyvfjFvO5QxGbJKy
     * CqNMVEIYFRIYvdr8unRu/8G2oGTYqV9Vrp9canaW2HNnh/tNf1zuacpzEPuKqf2e
     * vTY4SUmH9A4U8OmHuD+nT3pajnnUk+S7aFKErGzp85hwVXIy+TSrK0m1zSBi5Dp6
     * Z2Orltxtrpfs/J92VoguZs9btsmksNcFuuEnL5O7Jiqik7Ab846+HUCjuTaPPoIa
     * Gl6I6lD4WeKDRikL40Rc4ZW2aZCaFG+XroHPaO+Zmr615+F/+PoTRxZMzG0IQOeL
     * eG9QgkRQP2YGiqtDhFZKDyAthg710tvSeopLzaXoTvFeJiUBWSOgftL2fiFX1ye8
     * FVdMpEbB4IMeDExNH08GGeL5qPQ6gqGyeUN51q1veieQA6TqJIc/2b3Z6fJfUEkc
     * 7uzXLg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02NL1\x1e0\x1c\x06\x03U\x04\n\x0c\x15Staat der Nederlanden1)0\'\x06\x03U\x04\x03\x0c Staat der Nederlanden EV Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xe3\xc7~\x89\xf9$K:\xd23\x835,i\xec\xdc\t\xa4\xe3Q\xa8%+y\xb8\x08=\xe0\x91\xba\x84\x85\xc6\x85\xa4\xca\xe6\xc9.S\xa4\xc9$\x1e\xfdUfq],\xc5`h\x04\xb7\xd9\xc2R&8\x88\xa4\xd6;@\xa6\xc2\xcd?\xcd\x98\x93\xb3T\x14X\x96U\xd5P\xfe\x86\xad\xa4c\x7f\\\x87\xf6\x8e\xe6\'\x92g\x17\x92\x02\x03,\xdc\xd6ft\xed\xddg\xff\xc1a\x8dcO\x0f\x9bm\x170&\xef\xab\xd2\x1f\x10\xa0\xf9\xc5\x7f\x16i\x81\x03G\xed\x1eh\x8dr\xa1M\xb2&\xc6\xbal_m\xd6\xaf\xd1\xb1\x13\x8e\xa9\xad\xf3^iu&\x18>A+!\x7f\xee\x8b]\x07\x06\x9dC\xc4)\n+\xfc*>\x86\xcb<\x83:\xf9\xc9\r\xda\xc5\x99\xe2\xbcxA3v\xe1\xbf/]\xe5\xa4\x98P\x0c\x15\xdd\xe0\xfa\x9c\x7f8h\xd0\xb2\xa6z\xa7\xd11\xbd~\x8aX\'C\xb3\xba3\x91\xd3\xa7\x98\x15\\\x9a\xe6\xd3\x0fu\xd9\xfcA\x98\x97>\xaa%\xdb\x8f\x92.\xb0{\x0c_\xf1c\xa97\xf9\x9buiL(&%\xda\xd5\xf2\x12pEU\xe3\xdfs^7\xf5!l\x90\x8e5Z\xc9\xd3#\xeb\xd3\xc0\xbex\xacB(Xf\xa5Fmp\x02\xd7\x10\xf9KT\xfc]\x86J\x87\xcf\x7f\xcaE\xac\x11Z\xb5 Q\x8d/\x88G\x979\xc0\xcf\xba\xc0B\x01@\x99H!\x0bk\xa7\xd2\xfd\x96\xd5\xd1\xbeF\x9dI\xe0\x0b\xa6\xa0\"N8\xd0\xc1<0\xbcp\x8f,u\xcc\xd0\xc5\x8cQ;=\x94\x08d&a}\xb9\xc3e\x8f\x14\x9c!\xd0\xaa\xfd\x17r\x03\x8f\xbd\x9b\x8c\xe6^S\x9e\xb9\x9d\xef\x82\xbb\xe1\xbc\xe2rA[!\x94\xd3E7\x94\xd1\xdf\t9]\xe7#\xaa\x9a\x1d\xcam\xa8\n\x86\x85\x8a\x82\xbeB\x07\xd6\xf28\x82s\xda\x87[\xe5<\xd3\x9e>\xa7;\x9e\xf4\x03\xb3\xf9\xf1}\x13t\x02\xff\xbb\xa1\xe5\xfa\x00y\x1c\xa6fA\x88\\`W\xa6.\t\xc4\xba\xfd\x9a\xcf\xa7\x1f@\xc3\xbb\xccZ\nUK;8vQ\xb8c\x8b\x84\x94\x16\xe6V\xf3\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign Root R46 O=GlobalSign nv-sa
     * Subject: CN=GlobalSign Root R46 O=GlobalSign nv-sa
     * Label: "GlobalSign Root R46"
     * Serial: 1552617688466950547958867513931858518042577
     * MD5 Fingerprint: c4:14:30:e4:fa:66:43:94:2a:6a:1b:24:5f:19:d0:ef
     * SHA1 Fingerprint: 53:a2:b0:4b:ca:6b:d6:45:e6:39:8a:8e:c4:0d:d2:bf:77:c3:a2:90
     * SHA256 Fingerprint: 4f:a3:12:6d:8d:3a:11:d1:c4:85:5a:4f:80:7c:ba:d6:cf:91:9d:3a:5a:88:b0:3b:ea:2c:63:72:d9:3c:40:c9
     * -----BEGIN CERTIFICATE-----
     * MIIFWjCCA0KgAwIBAgISEdK7udcjGJ5AXwqdLdDfJWfRMA0GCSqGSIb3DQEBDAUA
     * MEYxCzAJBgNVBAYTAkJFMRkwFwYDVQQKExBHbG9iYWxTaWduIG52LXNhMRwwGgYD
     * VQQDExNHbG9iYWxTaWduIFJvb3QgUjQ2MB4XDTE5MDMyMDAwMDAwMFoXDTQ2MDMy
     * MDAwMDAwMFowRjELMAkGA1UEBhMCQkUxGTAXBgNVBAoTEEdsb2JhbFNpZ24gbnYt
     * c2ExHDAaBgNVBAMTE0dsb2JhbFNpZ24gUm9vdCBSNDYwggIiMA0GCSqGSIb3DQEB
     * AQUAA4ICDwAwggIKAoICAQCsrHQy6LNl5brtQyYdpokNRbopiLKkHWPd08EsCVeJ
     * OaFV6Wc0dwxu5FUdUiXSE2te4R2pt32JMl8Nnp8semNgQB+msLZ4j5lUlghYruQG
     * vGIFAha/r6gjA7aUD7xubMLL1aa7DOn2wQL7Id5m3RerdELv8HQvJfTqa1VbkNud
     * 316HCkD7rRlr+/fKYIje2sGP1q7Vf9Q8g+7XFkyDRTNrJ9CG0Bwta/OrffGFqfUo
     * 0q3v84RLHIf8E6M6cqJaESvWJ3En7YEtbWaBkoe0G1h6zD8K+kZPTXhc+CtI4wSE
     * y132tGqzZfxCnlEmIyDLPRT5ge1lFgBPGmSXZgjPjHvjK8Cd+RTyG/FWaha/LIWF
     * zXg4mutCagI0GIMXTpRW+LaCtfOW3T3zvn8gdz57GSNrLNRyc0NXfeD412lPFzYE
     * +cCQYDdF3uYM2HSNrpyibXRdQr4G9dlkbgIQrImwTDsHTUB+JMWKmIJ5jqSngiCN
     * I/onccnfxkF0oE32kRbcRoxfKWMxWXEM2G/CtjJ9++ZdU6Z+Ffy7dXxd7Pj2Fxzs
     * x2sZy/N78CsHpdlseVR2bJ0cpm4O6XkMqCNqo98bMDGfsVR7/mrLZqrcZdCinkqa
     * ByFrgY/bxFn63iLABJzjqls2k+g9vXqhnQt2sQvHnf3PmKgGwvgqo6GDoLclcqUC
     * 4wIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAYYwDwYDVR0TAQH/BAUwAwEB/zAdBgNV
     * HQ4EFgQUA1yrc4GHqMywptWU4jaWSf8FmSwwDQYJKoZIhvcNAQEMBQADggIBAHx4
     * 7PYCLLtbfpIrXTncvtgdokIzTfnvpCo7RGkerNlFo048p9gkUbJUHJNOxO97k4Vg
     * JuoJSOD1u8fpaNK7ajFxzHmuEajwmf3lH7wvqMxX63bEIaZHU1VNaL8FpO7XJqti
     * 2kM3S+LGteWygxk6x9PbTZ4IevPuzz5i+6zoYMzRx6Fcg0XERczzF2sUyQQCPtIk
     * pnnpHs6i58FZFZ8d4kuaPp92CC1r2LpXFNqD6v6MVenQTqnMdzGxRBF6XLE+0xRF
     * FRhiJBPSy03OXIPBNvIQtQ6IbbjhVp+J3pZmOUdkLG5NrmJ7v2B0GbhWrJKsFjLt
     * rWhV/pi60zTe9Mlhww6G9kuEYO4Ne7UyWHmRVSyBQ7N0H3qqJZ4d16GLuc1CLgSk
     * ZoNNiTW2bKg2SnkheCLQQrzRQDGQob4Ez8pn7fXwgNNgyYMqIgXQBztSvwyeqiv5
     * u+YfjyW6hY0XHgL+XVAEV8/+LbzvXMAaq7afJMbfc2hIkCwU9D9SGuTSyxTDYWnP
     * 4vkYxboznxSjBF25cfe1lNj2M8FawTSLfJvdkzrnE6JwYZ+vj+vYxXX4M2bUdGc6
     * N3ec592kD3ZDZopD8p/7DEJ4Y9HiD2971KE9dJeFt0g5QdYg/NA6s/rob8SKunE3
     * vouXsXgxT7PntgMTzlSdriVZzH81Xwj3QEUxeCp6
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BE1\x190\x17\x06\x03U\x04\n\x13\x10GlobalSign nv-sa1\x1c0\x1a\x06\x03U\x04\x03\x13\x13GlobalSign Root R46",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xac\xact2\xe8\xb3e\xe5\xba\xedC&\x1d\xa6\x89\rE\xba)\x88\xb2\xa4\x1dc\xdd\xd3\xc1,\tW\x899\xa1U\xe9g4w\x0cn\xe4U\x1dR%\xd2\x13k^\xe1\x1d\xa9\xb7}\x892_\r\x9e\x9f,zc`@\x1f\xa6\xb0\xb6x\x8f\x99T\x96\x08X\xae\xe4\x06\xbcb\x05\x02\x16\xbf\xaf\xa8#\x03\xb6\x94\x0f\xbcnl\xc2\xcb\xd5\xa6\xbb\x0c\xe9\xf6\xc1\x02\xfb!\xdef\xdd\x17\xabtB\xef\xf0t/%\xf4\xeakU[\x90\xdb\x9d\xdf^\x87\n@\xfb\xad\x19k\xfb\xf7\xca`\x88\xde\xda\xc1\x8f\xd6\xae\xd5\x7f\xd4<\x83\xee\xd7\x16L\x83E3k\'\xd0\x86\xd0\x1c-k\xf3\xab}\xf1\x85\xa9\xf5(\xd2\xad\xef\xf3\x84K\x1c\x87\xfc\x13\xa3:r\xa2Z\x11+\xd6\'q\'\xed\x81-mf\x81\x92\x87\xb4\x1bXz\xcc?\n\xfaFOMx\\\xf8+H\xe3\x04\x84\xcb]\xf6\xb4j\xb3e\xfcB\x9eQ&# \xcb=\x14\xf9\x81\xede\x16\x00O\x1ad\x97f\x08\xcf\x8c{\xe3+\xc0\x9d\xf9\x14\xf2\x1b\xf1Vj\x16\xbf,\x85\x85\xcdx8\x9a\xebBj\x024\x18\x83\x17N\x94V\xf8\xb6\x82\xb5\xf3\x96\xdd=\xf3\xbe\x7f w>{\x19#k,\xd4rsCW}\xe0\xf8\xd7iO\x176\x04\xf9\xc0\x90`7E\xde\xe6\x0c\xd8t\x8d\xae\x9c\xa2mt]B\xbe\x06\xf5\xd9dn\x02\x10\xac\x89\xb0L;\x07M@~$\xc5\x8a\x98\x82y\x8e\xa4\xa7\x82 \x8d#\xfa\'q\xc9\xdf\xc6At\xa0M\xf6\x91\x16\xdcF\x8c_)c1Yq\x0c\xd8o\xc2\xb62}\xfb\xe6]S\xa6~\x15\xfc\xbbu|]\xec\xf8\xf6\x17\x1c\xec\xc7k\x19\xcb\xf3{\xf0+\x07\xa5\xd9lyTvl\x9d\x1c\xa6n\x0e\xe9y\x0c\xa8#j\xa3\xdf\x1b01\x9f\xb1T{\xfej\xcbf\xaa\xdce\xd0\xa2\x9eJ\x9a\x07!k\x81\x8f\xdb\xc4Y\xfa\xde\"\xc0\x04\x9c\xe3\xaa[6\x93\xe8=\xbdz\xa1\x9d\x0bv\xb1\x0b\xc7\x9d\xfd\xcf\x98\xa8\x06\xc2\xf8*\xa3\xa1\x83\xa0\xb7%r\xa5\x02\xe3\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=USERTrust ECC Certification Authority O=The USERTRUST Network
     * Subject: CN=USERTrust ECC Certification Authority O=The USERTRUST Network
     * Label: "USERTrust ECC Certification Authority"
     * Serial: 123013823720199481456569720443997572134
     * MD5 Fingerprint: fa:68:bc:d9:b5:7f:ad:fd:c9:1d:06:83:28:cc:24:c1
     * SHA1 Fingerprint: d1:cb:ca:5d:b2:d5:2a:7f:69:3b:67:4d:e5:f0:5a:1d:0c:95:7d:f0
     * SHA256 Fingerprint: 4f:f4:60:d5:4b:9c:86:da:bf:bc:fc:57:12:e0:40:0d:2b:ed:3f:bc:4d:4f:bd:aa:86:e0:6a:dc:d2:a9:ad:7a
     * -----BEGIN CERTIFICATE-----
     * MIICjzCCAhWgAwIBAgIQXIuZxVqUxdJxVt7NiYDMJjAKBggqhkjOPQQDAzCBiDEL
     * MAkGA1UEBhMCVVMxEzARBgNVBAgTCk5ldyBKZXJzZXkxFDASBgNVBAcTC0plcnNl
     * eSBDaXR5MR4wHAYDVQQKExVUaGUgVVNFUlRSVVNUIE5ldHdvcmsxLjAsBgNVBAMT
     * JVVTRVJUcnVzdCBFQ0MgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMTAwMjAx
     * MDAwMDAwWhcNMzgwMTE4MjM1OTU5WjCBiDELMAkGA1UEBhMCVVMxEzARBgNVBAgT
     * Ck5ldyBKZXJzZXkxFDASBgNVBAcTC0plcnNleSBDaXR5MR4wHAYDVQQKExVUaGUg
     * VVNFUlRSVVNUIE5ldHdvcmsxLjAsBgNVBAMTJVVTRVJUcnVzdCBFQ0MgQ2VydGlm
     * aWNhdGlvbiBBdXRob3JpdHkwdjAQBgcqhkjOPQIBBgUrgQQAIgNiAAQarFRaqflo
     * I+d61SRvU8Za2EurxtW20eZzca7dnNYMYf3boIkDuAUU7FfO7l0/4iGzzvfUinng
     * o4N+LZfQYcTxmdwlkWOrfzCjtHDix6EznPO/LlxTsV+zfTJ/ijTjeXmjQjBAMB0G
     * A1UdDgQWBBQ64QmG1M8ZwpZ2dEl23OA1xmNjmjAOBgNVHQ8BAf8EBAMCAQYwDwYD
     * VR0TAQH/BAUwAwEB/zAKBggqhkjOPQQDAwNoADBlAjA2Z6EWCNzklwBBHU6+4WMB
     * zzuqQhFkoJ2UOQIReVx7Hfpkue4WQrO/isIJxOzksU0CMQDpKmFHjFJKS04YcPbW
     * RNZu9YO6bVi9JNlWSOrvxKJGgYhqOkbRqZtNyWHa0V1Xahg=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x130\x11\x06\x03U\x04\x08\x13\nNew Jersey1\x140\x12\x06\x03U\x04\x07\x13\x0bJersey City1\x1e0\x1c\x06\x03U\x04\n\x13\x15The USERTRUST Network1.0,\x06\x03U\x04\x03\x13%USERTrust ECC Certification Authority",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x1a\xacTZ\xa9\xf9h#\xe7z\xd5$oS\xc6Z\xd8K\xab\xc6\xd5\xb6\xd1\xe6sq\xae\xdd\x9c\xd6\x0ca\xfd\xdb\xa0\x89\x03\xb8\x05\x14\xecW\xce\xee]?\xe2!\xb3\xce\xf7\xd4\x8ay\xe0\xa3\x83~-\x97\xd0a\xc4\xf1\x99\xdc%\x91c\xab\x7f0\xa3\xb4p\xe2\xc7\xa13\x9c\xf3\xbf.\\S\xb1_\xb3}2\x7f\x8a4\xe3yy",
        name_constraints: None
    },

    /*
     * Issuer: O=SECOM Trust Systems CO.,LTD. OU=Security Communication RootCA2
     * Subject: O=SECOM Trust Systems CO.,LTD. OU=Security Communication RootCA2
     * Label: "Security Communication RootCA2"
     * Serial: 0
     * MD5 Fingerprint: 6c:39:7d:a4:0e:55:59:b2:3f:d6:41:b1:12:50:de:43
     * SHA1 Fingerprint: 5f:3b:8c:f2:f8:10:b3:7d:78:b4:ce:ec:19:19:c3:73:34:b9:c7:74
     * SHA256 Fingerprint: 51:3b:2c:ec:b8:10:d4:cd:e5:dd:85:39:1a:df:c6:c2:dd:60:d8:7b:b7:36:d2:b5:21:48:4a:a4:7a:0e:be:f6
     * -----BEGIN CERTIFICATE-----
     * MIIDdzCCAl+gAwIBAgIBADANBgkqhkiG9w0BAQsFADBdMQswCQYDVQQGEwJKUDEl
     * MCMGA1UEChMcU0VDT00gVHJ1c3QgU3lzdGVtcyBDTy4sTFRELjEnMCUGA1UECxMe
     * U2VjdXJpdHkgQ29tbXVuaWNhdGlvbiBSb290Q0EyMB4XDTA5MDUyOTA1MDAzOVoX
     * DTI5MDUyOTA1MDAzOVowXTELMAkGA1UEBhMCSlAxJTAjBgNVBAoTHFNFQ09NIFRy
     * dXN0IFN5c3RlbXMgQ08uLExURC4xJzAlBgNVBAsTHlNlY3VyaXR5IENvbW11bmlj
     * YXRpb24gUm9vdENBMjCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBANAV
     * OVKxUrO6xVmCxF1SrjpDZYBLx/KWvNs2l9amZIyoXvDjChz335c9S672XewhtUGr
     * zbl+dp+++T42NKA7wfYxEUV0kz1XgMX5iZnK5atq1LXaQZAQwdbWQonCv/Q4EpVM
     * VAX3NuRFg3sUZdbcDE3R3n4MqzvEFb46VqZab3ZpUql6ucjrappdUtAtCms1FgkQ
     * hNBqyjoGADdH5H5XTz+L62e4iKrFvlNVspHEfbmwhRkGeC7bYRr6hfVKkaHnFtWO
     * ojnflLhwHyg/i/xAXmODPIMqGplrz95Zajv8bxbXH/1KEOtOghY6rCcMU/Gt1SSw
     * awNQwS08Ft1ENCcadfsCAwEAAaNCMEAwHQYDVR0OBBYEFAqFqXdlBZh8QIH4D5cs
     * OPEK7DzPMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3
     * DQEBCwUAA4IBAQBMOqNErLlFsceTfsgLCkLfZOoc7llsCLqJX2rKSpWeeo8HxdpF
     * coJxDjrSzG+ntKEju/Ykn8sX/oymzsLS28yN/HH8AynBbF0zX2S2ZTuJbxh2ePXc
     * okgfGT+Ok+vx+hfuzU7jBBJV1uXk3fs+BXziHV7Gp7yXT2g69ekuCkO2r1dcYmh8
     * t/2jioSgrGK+KwmHNPBqAbubKVY8/gA3zyNs8U6qtnRGEmyR7jTV7JqR50S+kDFy
     * 1UkC9gLl9B/rfNmWVan/7Ir5mUf/NVoCqgTLiluHcSmRvaS0eg29mvVXIwAHIRc/
     * SjnRBUkLp7Y3gaVdjKozXoEofKd9J+sAro03
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02JP1%0#\x06\x03U\x04\n\x13\x1cSECOM Trust Systems CO.,LTD.1\'0%\x06\x03U\x04\x0b\x13\x1eSecurity Communication RootCA2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd0\x159R\xb1R\xb3\xba\xc5Y\x82\xc4]R\xae:Ce\x80K\xc7\xf2\x96\xbc\xdb6\x97\xd6\xa6d\x8c\xa8^\xf0\xe3\n\x1c\xf7\xdf\x97=K\xae\xf6]\xec!\xb5A\xab\xcd\xb9~v\x9f\xbe\xf9>64\xa0;\xc1\xf61\x11Et\x93=W\x80\xc5\xf9\x89\x99\xca\xe5\xabj\xd4\xb5\xdaA\x90\x10\xc1\xd6\xd6B\x89\xc2\xbf\xf48\x12\x95LT\x05\xf76\xe4E\x83{\x14e\xd6\xdc\x0cM\xd1\xde~\x0c\xab;\xc4\x15\xbe:V\xa6ZoviR\xa9z\xb9\xc8\xebj\x9a]R\xd0-\nk5\x16\t\x10\x84\xd0j\xca:\x06\x007G\xe4~WO?\x8b\xebg\xb8\x88\xaa\xc5\xbeSU\xb2\x91\xc4}\xb9\xb0\x85\x19\x06x.\xdba\x1a\xfa\x85\xf5J\x91\xa1\xe7\x16\xd5\x8e\xa29\xdf\x94\xb8p\x1f(?\x8b\xfc@^c\x83<\x83*\x1a\x99k\xcf\xdeYj;\xfco\x16\xd7\x1f\xfdJ\x10\xebN\x82\x16:\xac\'\x0cS\xf1\xad\xd5$\xb0k\x03P\xc1-<\x16\xddD4\'\x1au\xfb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=COMODO RSA Certification Authority O=COMODO CA Limited
     * Subject: CN=COMODO RSA Certification Authority O=COMODO CA Limited
     * Label: "COMODO RSA Certification Authority"
     * Serial: 101909084537582093308941363524873193117
     * MD5 Fingerprint: 1b:31:b0:71:40:36:cc:14:36:91:ad:c4:3e:fd:ec:18
     * SHA1 Fingerprint: af:e5:d2:44:a8:d1:19:42:30:ff:47:9f:e2:f8:97:bb:cd:7a:8c:b4
     * SHA256 Fingerprint: 52:f0:e1:c4:e5:8e:c6:29:29:1b:60:31:7f:07:46:71:b8:5d:7e:a8:0d:5b:07:27:34:63:53:4b:32:b4:02:34
     * -----BEGIN CERTIFICATE-----
     * MIIF2DCCA8CgAwIBAgIQTKr5yttjb+Af907YWwOGnTANBgkqhkiG9w0BAQwFADCB
     * hTELMAkGA1UEBhMCR0IxGzAZBgNVBAgTEkdyZWF0ZXIgTWFuY2hlc3RlcjEQMA4G
     * A1UEBxMHU2FsZm9yZDEaMBgGA1UEChMRQ09NT0RPIENBIExpbWl0ZWQxKzApBgNV
     * BAMTIkNPTU9ETyBSU0EgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMTAwMTE5
     * MDAwMDAwWhcNMzgwMTE4MjM1OTU5WjCBhTELMAkGA1UEBhMCR0IxGzAZBgNVBAgT
     * EkdyZWF0ZXIgTWFuY2hlc3RlcjEQMA4GA1UEBxMHU2FsZm9yZDEaMBgGA1UEChMR
     * Q09NT0RPIENBIExpbWl0ZWQxKzApBgNVBAMTIkNPTU9ETyBSU0EgQ2VydGlmaWNh
     * dGlvbiBBdXRob3JpdHkwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCR
     * 6FSS0gpWsawNJN3Fz0RndJkrN6N9I3AAcbxT38T6KhKPS38QVr2fcHK3YX/JSw8X
     * pz3jsARh7v8Rl8f0hj4K+j5c+ZPmNHrZFGvnnLOFoIJ6dq9xkNfs/Q36nGz637CC
     * 9BR++b7Epi9Pf5l/tfxnQ3K9DADWietrLNPtj5gcFKt+5eNu/Nio5JIk2kNrYrhV
     * /erBvGy2i/MOjZrkm2xpmfh4SDBF1a3hDTxFYPwyllEnvGfDyi62a+pGx8cgoLEf
     * Zd5ICLqkTqnyg0Y3hOvozIFIQ2dOciqbXL1MGyiKXCJ7tKuY2e7gUYPDCUZObT6Z
     * +pUX2nwzV0E8jVHtC7ZcryxjGt9XyD+86V3Em69FmeKjWiS0uqlWPc9vqv9JWL7w
     * qP/0uK3pN/u6uPQLOvnoQ0IeidiEyxPx2bvhiWC4jChWrBQdnArncevPDt09qZah
     * SL0896+1DSJMwBGB7FY79tOi4lu3sgQiUpWAk2nojkxl8ZEDLXB0AuqLZxUpaVIC
     * u9ffUGpVRr+goyhhf3DQw6KqLCGqR84onAZFdr+CGCe01a60y1Dma/RMhnEw6abf
     * Fobg2P9A3fvQQoh/ozM6LlweQRGBY84YcWsr7KaKtzFcOmpH4MN5WdYgGq/yapiq
     * crxXStJLnbsQ/LBMQeXtHT1eKJ2czL+zUdqnR+WEUwIDAQABo0IwQDAdBgNVHQ4E
     * FgQUu69+Aj36pvE8hI6t7jiY7NkyMtQwDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB
     * /wQFMAMBAf8wDQYJKoZIhvcNAQEMBQADggIBAArx1UaEt65Ru2yyTUEUAJNMnMvl
     * wFTPoCWOAvn9sKIN9SCYPBMtrFaisNZ+EZLpLrqeLppysb0ZRGxhNaKatBYSaVqM
     * 4dc+pBroLwP0rmEdEBsqpIt6xf4FpuHA1sj+nq6PK7o9mfjYcwlYRm6mnPTXJ9OV
     * 2jeDchzTc+CiR5kDOF3VSXkAKRzH7JsgHAckaVd4sjn8OoSgtZx8jb8uk2Intzna
     * FxiuvTwJaP+EmzzV1gsD41eeFPfR60/IvYcjt7ZJQ3mFXLrrkguhxuhoqEwWsRqZ
     * CuhTLJK7oQkYdQxlqHvLI7cawiiFwxv/0Cti76R7CZGYZ4wUAc1oBmpjIXUDgIiK
     * boHGhfKppC3n9KUkEEeDys30jXlYsQab5xoq2Z0B15R97QNKyvDb6KkBPvVWmcke
     * jkk9u+UJueBPSZI9FoJAzMxZxuY67RIuaTxslbH9qh17f4a+Hg4yRvv7E491f0yL
     * S0Zj/gA0QHDBw7mh3aZw4gSzQbzpgJHqZJx64SIDqZxubw5lT2yHh17zbqD5daWb
     * QOhTsiedSrnAdyGN/4fy3ryM7xfft0kL0fJuMAsaDk527RH89elWsn2/x20Kk4yl
     * 0MC2Hb46TpSi125sC8KKfPog88Tk5c0NqMuRkrF8hey1FGlmDoLnzc7ILaZRfyHB
     * NVOFBkpdn627G190
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GB1\x1b0\x19\x06\x03U\x04\x08\x13\x12Greater Manchester1\x100\x0e\x06\x03U\x04\x07\x13\x07Salford1\x1a0\x18\x06\x03U\x04\n\x13\x11COMODO CA Limited1+0)\x06\x03U\x04\x03\x13\"COMODO RSA Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x91\xe8T\x92\xd2\nV\xb1\xac\r$\xdd\xc5\xcfDgt\x99+7\xa3}#p\x00q\xbcS\xdf\xc4\xfa*\x12\x8fK\x7f\x10V\xbd\x9fpr\xb7a\x7f\xc9K\x0f\x17\xa7=\xe3\xb0\x04a\xee\xff\x11\x97\xc7\xf4\x86>\n\xfa>\\\xf9\x93\xe64z\xd9\x14k\xe7\x9c\xb3\x85\xa0\x82zv\xafq\x90\xd7\xec\xfd\r\xfa\x9cl\xfa\xdf\xb0\x82\xf4\x14~\xf9\xbe\xc4\xa6/O\x7f\x99\x7f\xb5\xfcgCr\xbd\x0c\x00\xd6\x89\xebk,\xd3\xed\x8f\x98\x1c\x14\xab~\xe5\xe3n\xfc\xd8\xa8\xe4\x92$\xdaCkb\xb8U\xfd\xea\xc1\xbcl\xb6\x8b\xf3\x0e\x8d\x9a\xe4\x9bli\x99\xf8xH0E\xd5\xad\xe1\r<E`\xfc2\x96Q\'\xbcg\xc3\xca.\xb6k\xeaF\xc7\xc7 \xa0\xb1\x1fe\xdeH\x08\xba\xa4N\xa9\xf2\x83F7\x84\xeb\xe8\xcc\x81HCgNr*\x9b\\\xbdL\x1b(\x8a\\\"{\xb4\xab\x98\xd9\xee\xe0Q\x83\xc3\tFNm>\x99\xfa\x95\x17\xda|3WA<\x8dQ\xed\x0b\xb6\\\xaf,c\x1a\xdfW\xc8?\xbc\xe9]\xc4\x9b\xafE\x99\xe2\xa3Z$\xb4\xba\xa9V=\xcfo\xaa\xffIX\xbe\xf0\xa8\xff\xf4\xb8\xad\xe97\xfb\xba\xb8\xf4\x0b:\xf9\xe8CB\x1e\x89\xd8\x84\xcb\x13\xf1\xd9\xbb\xe1\x89`\xb8\x8c(V\xac\x14\x1d\x9c\n\xe7q\xeb\xcf\x0e\xdd=\xa9\x96\xa1H\xbd<\xf7\xaf\xb5\r\"L\xc0\x11\x81\xecV;\xf6\xd3\xa2\xe2[\xb7\xb2\x04\"R\x95\x80\x93i\xe8\x8eLe\xf1\x91\x03-pt\x02\xea\x8bg\x15)iR\x02\xbb\xd7\xdfPjUF\xbf\xa0\xa3(a\x7fp\xd0\xc3\xa2\xaa,!\xaaG\xce(\x9c\x06Ev\xbf\x82\x18\'\xb4\xd5\xae\xb4\xcbP\xe6k\xf4L\x86q0\xe9\xa6\xdf\x16\x86\xe0\xd8\xff@\xdd\xfb\xd0B\x88\x7f\xa33:.\\\x1eA\x11\x81c\xce\x18qk+\xec\xa6\x8a\xb71\\:jG\xe0\xc3yY\xd6 \x1a\xaf\xf2j\x98\xaar\xbcWJ\xd2K\x9d\xbb\x10\xfc\xb0LA\xe5\xed\x1d=^(\x9d\x9c\xcc\xbf\xb3Q\xda\xa7G\xe5\x84S\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Trusted Root G4 O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Trusted Root G4 O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Trusted Root G4"
     * Serial: 7451500558977370777930084869016614236
     * MD5 Fingerprint: 78:f2:fc:aa:60:1f:2f:b4:eb:c9:37:ba:53:2e:75:49
     * SHA1 Fingerprint: dd:fb:16:cd:49:31:c9:73:a2:03:7d:3f:c8:3a:4d:7d:77:5d:05:e4
     * SHA256 Fingerprint: 55:2f:7b:dc:f1:a7:af:9e:6c:e6:72:01:7f:4f:12:ab:f7:72:40:c7:8e:76:1a:c2:03:d1:d9:d2:0a:c8:99:88
     * -----BEGIN CERTIFICATE-----
     * MIIFkDCCA3igAwIBAgIQBZsbV56OITLiOQe9p3d1XDANBgkqhkiG9w0BAQwFADBi
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSEwHwYDVQQDExhEaWdpQ2VydCBUcnVzdGVkIFJvb3Qg
     * RzQwHhcNMTMwODAxMTIwMDAwWhcNMzgwMTE1MTIwMDAwWjBiMQswCQYDVQQGEwJV
     * UzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cuZGlnaWNlcnQu
     * Y29tMSEwHwYDVQQDExhEaWdpQ2VydCBUcnVzdGVkIFJvb3QgRzQwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQC/5pBzaN675F1KPDAiMGkz7MKnJS7JIT3y
     * ithZwuEppz1Yq3aaza57G4QNxDAf8xukOBbrVsaXbR2rsnnyyhHS5F/WBTxSD1If
     * xp4VpX6+n6lXFllVcq9ok3DCsrp1mWpzMpTREEQQLt+C8weE5nQ7bXHiLQwb7iDV
     * ySAdYyktzuxeTsiT+CFhmzTrBcZe7FsavOvJz82sNEBfsXpm7nfISKhmV1efVFiO
     * DCu3T6cw2Vbuyntd463JT17lNecxy9qTXtyOj4DatpGYQJB5w3jHtrHEtWoYOAMQ
     * jdjUN6QuBX2I9YI+EJFwq1WCQTLX2wRzKm6RAXwhTNS8rhsDdV14Ztk6MUSaM0C/
     * CNdaSaTC5qmgZ92kJ7yhTzm1EVgX9yRcRo9k98FpiHaYdj1ZXUJ2h4mXaXpI8OCi
     * EhtmmnTK3kse5w5jrubU75KSOp493ADkRSWJtppEGSt+wJS00mFt6zPZxd9LBADM
     * fRyVw4/3IbKyEbe7f/LVjHAsQWCqsWMYRJUadmJ+9oCw++hkpjPRiQfhvbfmQ6QY
     * uKZ3AeEPlAwhHbJUKSWJbOUOUlFHdL4mrLZBdd56rF+NP8m800ERElvlEFDrMcXK
     * chYiCd98THU/Y+whX8QgUWtvsauGi0/C1kVfnSD8oR7FwI+isX4KJpn15GkvmB0t
     * 9dmpsh3lGwIDAQABo0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIB
     * hjAdBgNVHQ4EFgQU7NfjgtJxXWRM3y5nP+e6mK4cD08wDQYJKoZIhvcNAQEMBQAD
     * ggIBALth2X2pbL4XxJEbw6GiAI3jZGgPVs93rnD5/ZpKmbnJeFwMDF/k5hQpVgs2
     * SV1EY+CtnJYYZhsjDT156W1r1lT40jzBQ0CuHVD1UvyQO7uYmWlrx8GnqGikJ9yd
     * +SeuMIW59mdNOj6PWTkiU0TryF0Dyu1Qen1iIQqAyHNm0aAFYF/opbSnr6j3bTWc
     * fFqK1qI4mfN4i/RN0iAL3gTujJtHgXINwBQy7zBZLq7gcfJW5GqXb5JQbZaNaHqa
     * sjYUegbyJLkJEVDXCLG4iXqEI2FCKeWjzaIgQdfRnGTZ6iahixTXTBmyUEFxPT9N
     * cCOGDErcgdLMMpSEDQgJlxxPwO5rIHQw0uA5NBCFIRUBCOhVMt5xSdkoF1BN5r5N
     * 0XWs0Mr7QbhDparTwwVETyw2m+L64kW4I1NsBm9nVX9GtUw/bihaeSbSpKhil9Ie
     * 4u1Ki7wb/UdKDd9nZn6yW0HQO+T0O/QEY+nvwlQAUaCKKsnOeMzV6ocEGLPOr0mI
     * r/OSmbaz5mEP0oUA51Aa5BuVnRmhuZyxm7EAHu/QD09CbMkKvO5D+jpxpchNJqU1
     * /YldvIViHTLSoCtU7ZpXwdv6EM8Zt4tKG48BtieVU+i2iW1bvGjUI+iLUaJW+fCm
     * gKDWHrO8Dw9TdSmq6hN35N6MgSGtBxBHEa2HPQfRdbzP82Z+
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1!0\x1f\x06\x03U\x04\x03\x13\x18DigiCert Trusted Root G4",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xbf\xe6\x90sh\xde\xbb\xe4]J<0\"0i3\xec\xc2\xa7%.\xc9!=\xf2\x8a\xd8Y\xc2\xe1)\xa7=X\xabv\x9a\xcd\xae{\x1b\x84\r\xc40\x1f\xf3\x1b\xa48\x16\xebV\xc6\x97m\x1d\xab\xb2y\xf2\xca\x11\xd2\xe4_\xd6\x05<R\x0fR\x1f\xc6\x9e\x15\xa5~\xbe\x9f\xa9W\x16YUr\xafh\x93p\xc2\xb2\xbau\x99js2\x94\xd1\x10D\x10.\xdf\x82\xf3\x07\x84\xe6t;mq\xe2-\x0c\x1b\xee \xd5\xc9 \x1dc)-\xce\xec^N\xc8\x93\xf8!a\x9b4\xeb\x05\xc6^\xec[\x1a\xbc\xeb\xc9\xcf\xcd\xac4@_\xb1zf\xeew\xc8H\xa8fWW\x9fTX\x8e\x0c+\xb7O\xa70\xd9V\xee\xca{]\xe3\xad\xc9O^\xe55\xe71\xcb\xda\x93^\xdc\x8e\x8f\x80\xda\xb6\x91\x98@\x90y\xc3x\xc7\xb6\xb1\xc4\xb5j\x188\x03\x10\x8d\xd8\xd47\xa4.\x05}\x88\xf5\x82>\x10\x91p\xabU\x82A2\xd7\xdb\x04s*n\x91\x01|!L\xd4\xbc\xae\x1b\x03u]xf\xd9:1D\x9a3@\xbf\x08\xd7ZI\xa4\xc2\xe6\xa9\xa0g\xdd\xa4\'\xbc\xa1O9\xb5\x11X\x17\xf7$\\F\x8fd\xf7\xc1i\x88v\x98v=Y]Bv\x87\x89\x97izH\xf0\xe0\xa2\x12\x1bf\x9at\xca\xdeK\x1e\xe7\x0ec\xae\xe6\xd4\xef\x92\x92:\x9e=\xdc\x00\xe4E%\x89\xb6\x9aD\x19+~\xc0\x94\xb4\xd2am\xeb3\xd9\xc5\xdfK\x04\x00\xcc}\x1c\x95\xc3\x8f\xf7!\xb2\xb2\x11\xb7\xbb\x7f\xf2\xd5\x8cp,A`\xaa\xb1c\x18D\x95\x1avb~\xf6\x80\xb0\xfb\xe8d\xa63\xd1\x89\x07\xe1\xbd\xb7\xe6C\xa4\x18\xb8\xa6w\x01\xe1\x0f\x94\x0c!\x1d\xb2T)%\x89l\xe5\x0eRQGt\xbe&\xac\xb6Au\xdez\xac_\x8d?\xc9\xbc\xd3A\x11\x12[\xe5\x10P\xeb1\xc5\xcar\x16\"\t\xdf|Lu?c\xec!_\xc4 Qko\xb1\xab\x86\x8bO\xc2\xd6E_\x9d \xfc\xa1\x1e\xc5\xc0\x8f\xa2\xb1~\n&\x99\xf5\xe4i/\x98\x1d-\xf5\xd9\xa9\xb2\x1d\xe5\x1b\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=AC RAIZ FNMT-RCM SERVIDORES SEGUROS O=FNMT-RCM OU=Ceres
     * Subject: CN=AC RAIZ FNMT-RCM SERVIDORES SEGUROS O=FNMT-RCM OU=Ceres
     * Label: "AC RAIZ FNMT-RCM SERVIDORES SEGUROS"
     * Serial: 131542671362353147877283741781055151509
     * MD5 Fingerprint: 19:36:9c:52:03:2f:d2:d1:bb:23:cc:dd:1e:12:55:bb
     * SHA1 Fingerprint: 62:ff:d9:9e:c0:65:0d:03:ce:75:93:d2:ed:3f:2d:32:c9:e3:e5:4a
     * SHA256 Fingerprint: 55:41:53:b1:3d:2c:f9:dd:b7:53:bf:be:1a:4e:0a:e0:8d:0a:a4:18:70:58:fe:60:a2:b8:62:b2:e4:b8:7b:cb
     * -----BEGIN CERTIFICATE-----
     * MIICbjCCAfOgAwIBAgIQYvYybOXE42hcG2LdnC6dlTAKBggqhkjOPQQDAzB4MQsw
     * CQYDVQQGEwJFUzERMA8GA1UECgwIRk5NVC1SQ00xDjAMBgNVBAsMBUNlcmVzMRgw
     * FgYDVQRhDA9WQVRFUy1RMjgyNjAwNEoxLDAqBgNVBAMMI0FDIFJBSVogRk5NVC1S
     * Q00gU0VSVklET1JFUyBTRUdVUk9TMB4XDTE4MTIyMDA5MzczM1oXDTQzMTIyMDA5
     * MzczM1oweDELMAkGA1UEBhMCRVMxETAPBgNVBAoMCEZOTVQtUkNNMQ4wDAYDVQQL
     * DAVDZXJlczEYMBYGA1UEYQwPVkFURVMtUTI4MjYwMDRKMSwwKgYDVQQDDCNBQyBS
     * QUlaIEZOTVQtUkNNIFNFUlZJRE9SRVMgU0VHVVJPUzB2MBAGByqGSM49AgEGBSuB
     * BAAiA2IABPa6V1PIyqvfNkpSIeSX0oNnnvBlUdBeh8dHsVnyV0ebAAKTRBdp20LH
     * sbI6GA60XYyzZl2hNPk2LEnb80b8s0RpRBNm/dfF/a82Tc4DTQdxz69qBdKiQ1oK
     * Um8BA06Oi6NCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYD
     * VR0OBBYEFAG5L++/EYZg8k/QQW6rcx/n0m5JMAoGCCqGSM49BAMDA2kAMGYCMQCu
     * SuMrQMN0EfKVrRYj3k4MGuZdpSRea0R7/DjiT8ucRRcRTBQnJlU5dUoDzBOQn5IC
     * MQD6SmxgiHPz7riYYqnOK8LZiqZwMR2vsJRM60/G49HzYqc8/5MuB1xJAWdpEgJy
     * v+c=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02ES1\x110\x0f\x06\x03U\x04\n\x0c\x08FNMT-RCM1\x0e0\x0c\x06\x03U\x04\x0b\x0c\x05Ceres1\x180\x16\x06\x03U\x04a\x0c\x0fVATES-Q2826004J1,0*\x06\x03U\x04\x03\x0c#AC RAIZ FNMT-RCM SERVIDORES SEGUROS",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xf6\xbaWS\xc8\xca\xab\xdf6JR!\xe4\x97\xd2\x83g\x9e\xf0eQ\xd0^\x87\xc7G\xb1Y\xf2WG\x9b\x00\x02\x93D\x17i\xdbB\xc7\xb1\xb2:\x18\x0e\xb4]\x8c\xb3f]\xa14\xf96,I\xdb\xf3F\xfc\xb3DiD\x13f\xfd\xd7\xc5\xfd\xaf6M\xce\x03M\x07q\xcf\xafj\x05\xd2\xa2CZ\nRo\x01\x03N\x8e\x8b",
        name_constraints: None
    },

    /*
     * Issuer: CN=Trustwave Global ECC P384 Certification Authority O=Trustwave Holdings, Inc.
     * Subject: CN=Trustwave Global ECC P384 Certification Authority O=Trustwave Holdings, Inc.
     * Label: "Trustwave Global ECC P384 Certification Authority"
     * Serial: 2704997926503831671788816187
     * MD5 Fingerprint: ea:cf:60:c4:3b:b9:15:29:40:a1:97:ed:78:27:93:d6
     * SHA1 Fingerprint: e7:f3:a3:c8:cf:6f:c3:04:2e:6d:0e:67:32:c5:9e:68:95:0d:5e:d2
     * SHA256 Fingerprint: 55:90:38:59:c8:c0:c3:eb:b8:75:9e:ce:4e:25:57:22:5f:f5:75:8b:bd:38:eb:d4:82:76:60:1e:1b:d5:80:97
     * -----BEGIN CERTIFICATE-----
     * MIICnTCCAiSgAwIBAgIMCL2Fl2yZJ6SAaEc7MAoGCCqGSM49BAMDMIGRMQswCQYD
     * VQQGEwJVUzERMA8GA1UECBMISWxsaW5vaXMxEDAOBgNVBAcTB0NoaWNhZ28xITAf
     * BgNVBAoTGFRydXN0d2F2ZSBIb2xkaW5ncywgSW5jLjE6MDgGA1UEAxMxVHJ1c3R3
     * YXZlIEdsb2JhbCBFQ0MgUDM4NCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTAeFw0x
     * NzA4MjMxOTM2NDNaFw00MjA4MjMxOTM2NDNaMIGRMQswCQYDVQQGEwJVUzERMA8G
     * A1UECBMISWxsaW5vaXMxEDAOBgNVBAcTB0NoaWNhZ28xITAfBgNVBAoTGFRydXN0
     * d2F2ZSBIb2xkaW5ncywgSW5jLjE6MDgGA1UEAxMxVHJ1c3R3YXZlIEdsb2JhbCBF
     * Q0MgUDM4NCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTB2MBAGByqGSM49AgEGBSuB
     * BAAiA2IABGvaDXU1CDFHBa5FmVXxERMuSvgQMSOjfoPTfygIOiYaOs+Xgh+AtycJ
     * j9GOMMQKmw6sWASr9zZ9lCOkmwqKi6vr/TklZvFe/oyujUF5nQlgziip04pt89ZF
     * 1PKYhDhloKNDMEEwDwYDVR0TAQH/BAUwAwEB/zAPBgNVHQ8BAf8EBQMDBwYAMB0G
     * A1UdDgQWBBRVqYSJ0sEyvRjLbKYHTsjnnb6CkDAKBggqhkjOPQQDAwNnADBkAjA3
     * AZKXRRJ+oPM+rRk6ct30UJMDEr5E0k9BpIycnR+j9sKS50gU/k6bpZFXrsY3crsC
     * MGclCrEMXu6pY5Jv5ZAL/mYiykf9ijH3g/56vxC+GCsej/YpHpRZ744hN8tRmKVu
     * Sw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x110\x0f\x06\x03U\x04\x08\x13\x08Illinois1\x100\x0e\x06\x03U\x04\x07\x13\x07Chicago1!0\x1f\x06\x03U\x04\n\x13\x18Trustwave Holdings, Inc.1:08\x06\x03U\x04\x03\x131Trustwave Global ECC P384 Certification Authority",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04k\xda\ru5\x081G\x05\xaeE\x99U\xf1\x11\x13.J\xf8\x101#\xa3~\x83\xd3\x7f(\x08:&\x1a:\xcf\x97\x82\x1f\x80\xb7\'\t\x8f\xd1\x8e0\xc4\n\x9b\x0e\xacX\x04\xab\xf76}\x94#\xa4\x9b\n\x8a\x8b\xab\xeb\xfd9%f\xf1^\xfe\x8c\xae\x8dAy\x9d\t`\xce(\xa9\xd3\x8am\xf3\xd6E\xd4\xf2\x98\x848e\xa0",
        name_constraints: None
    },

    /*
     * Issuer: CN=Actalis Authentication Root CA O=Actalis S.p.A./03358520967
     * Subject: CN=Actalis Authentication Root CA O=Actalis S.p.A./03358520967
     * Label: "Actalis Authentication Root CA"
     * Serial: 6271844772424770508
     * MD5 Fingerprint: 69:c1:0d:4f:07:a3:1b:c3:fe:56:3d:04:bc:11:f6:a6
     * SHA1 Fingerprint: f3:73:b3:87:06:5a:28:84:8a:f2:f3:4a:ce:19:2b:dd:c7:8e:9c:ac
     * SHA256 Fingerprint: 55:92:60:84:ec:96:3a:64:b9:6e:2a:be:01:ce:0b:a8:6a:64:fb:fe:bc:c7:aa:b5:af:c1:55:b3:7f:d7:60:66
     * -----BEGIN CERTIFICATE-----
     * MIIFuzCCA6OgAwIBAgIIVwoRl0LE48wwDQYJKoZIhvcNAQELBQAwazELMAkGA1UE
     * BhMCSVQxDjAMBgNVBAcMBU1pbGFuMSMwIQYDVQQKDBpBY3RhbGlzIFMucC5BLi8w
     * MzM1ODUyMDk2NzEnMCUGA1UEAwweQWN0YWxpcyBBdXRoZW50aWNhdGlvbiBSb290
     * IENBMB4XDTExMDkyMjExMjIwMloXDTMwMDkyMjExMjIwMlowazELMAkGA1UEBhMC
     * SVQxDjAMBgNVBAcMBU1pbGFuMSMwIQYDVQQKDBpBY3RhbGlzIFMucC5BLi8wMzM1
     * ODUyMDk2NzEnMCUGA1UEAwweQWN0YWxpcyBBdXRoZW50aWNhdGlvbiBSb290IENB
     * MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAp8bEpSmkLO/lGMWwUKNv
     * UTufClrJwkg4CsIcoBh/kbWHuUA/3R1oHwiD1S0eiKD4j1aPbZkCkpAW1V8IbInX
     * 4ay8IMKx4INRimlNAJZaby/ARH6jDuSRzVju3PvHHkVH3Se5CAGfpiEd9UEtL0z9
     * KK3giq0itFZljoZUj5NDKd45RnijMCO6zfB9E1fAXdKDa0hMxKufgFpbOr3JpyI/
     * gCczWw63igxdBzcIy2zSekciRDXFzMwujt0q7bd9Zg1fYVEiVRvjRuPjPdA1Yprb
     * rxTIW6HMiRvhMCb8oJsfgadHHwTrozmSBp+Z07/T6k9QnBn+locePGX2oxgkg4YQ
     * 51Q+qDp2JE+BIcXjDwL4k5RHILv+1A7TaLndxHqEguNTVHnd25zS8gebLra8Pu2F
     * be8lEfKXGkJh90qX6IuxEAf6ZYGyojnP9zz/GPvG8VqLWeICrHuS0E4UT1lF9gxe
     * KF+w6D9Fz8+vm2/7hNN3WpVvrJSEnu68wEqPSpP4RCHiMUVhUE4Q2OM1fEwZtN4F
     * v6MGn8i1zeQf1xcGDXqVdFUNaBr8EBtiZJ1t4JWgw5QHVw0U5r0F+7if5t+L4sbn
     * fpb2U8WANFAoWPASUHEXMLrmeGO89LKtmyuy/uE5jF66CyCU3nuDuP/jVo23Eek7
     * jPKxwV2dpAtMK9myGPW1n0sCAwEAAaNjMGEwHQYDVR0OBBYEFFLYiDrIn3hm7Ynz
     * ezhwlMkCAjbQMA8GA1UdEwEB/wQFMAMBAf8wHwYDVR0jBBgwFoAUUtiIOsifeGbt
     * ifN7OHCUyQICNtAwDgYDVR0PAQH/BAQDAgEGMA0GCSqGSIb3DQEBCwUAA4ICAQAL
     * e3KHwGCmSUyIWOYdiPcUZEim2FgKDk8TNd81HdTtBjHIgT5q1d07GjLukD0R0i70
     * jsNjLiNmsGe+b7bAEzlgqqI0JZN1Ut6nna0Oh4lScWoWPBkdg/iaKWW+9D+a2fDz
     * WochcYBNy+A4mz+7+uAwTc+G02UQGRjRlwKxK3JCaKygvU5a2hi/a5iB0P2avl4V
     * SM0RFbnAKVy06Ij3Pjaut2L9HmLecHgQHEhb2rykOLpn7VU+Xlff1ANATIGk0k9j
     * pwlCCRT8AKnCgHNPLsBA2RF7SOp6AsDT6ygBJlh0wcBzIm2Tlf05fbsq4/aC4yyX
     * X04fkZT6/iyj2HYauE2yOE+b+h1IYHkm4vP9qdCa6HCPSXrW5b0KDtst842/6+Ok
     * fcvHlXHo2qN8xcL4dJIEG4aspCJTQLas/kx2z/uUMsA1n3Y/buWQbqCmJqK4LL7R
     * K4X9p2jIugErsWx0Hbhzlefut8cl8ABMALJ+tguLHPPAUJ4lueAI3jZm/zel0btU
     * ZCzJJ7VLkn5l/9Mt4blOvH+kQSGQQXemOR/qnuOf0GZvBeyqdn6/axag67XH/JJU
     * LysRJyU3eExRarDzzFhdFPFqSBX/wge2sY0PjlxQRrM9vwGYT7JZVEc+NHt4bVaT
     * LnPqZih4zR0Uv6CPLy64Lo7yFIrM6bV8+2ydDKXhlg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02IT1\x0e0\x0c\x06\x03U\x04\x07\x0c\x05Milan1#0!\x06\x03U\x04\n\x0c\x1aActalis S.p.A./033585209671\'0%\x06\x03U\x04\x03\x0c\x1eActalis Authentication Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa7\xc6\xc4\xa5)\xa4,\xef\xe5\x18\xc5\xb0P\xa3oQ;\x9f\nZ\xc9\xc2H8\n\xc2\x1c\xa0\x18\x7f\x91\xb5\x87\xb9@?\xdd\x1dh\x1f\x08\x83\xd5-\x1e\x88\xa0\xf8\x8fV\x8fm\x99\x02\x92\x90\x16\xd5_\x08l\x89\xd7\xe1\xac\xbc \xc2\xb1\xe0\x83Q\x8aiM\x00\x96Zo/\xc0D~\xa3\x0e\xe4\x91\xcdX\xee\xdc\xfb\xc7\x1eEG\xdd\'\xb9\x08\x01\x9f\xa6!\x1d\xf5A-/L\xfd(\xad\xe0\x8a\xad\"\xb4Ve\x8e\x86T\x8f\x93C)\xde9Fx\xa30#\xba\xcd\xf0}\x13W\xc0]\xd2\x83kHL\xc4\xab\x9f\x80Z[:\xbd\xc9\xa7\"?\x80\'3[\x0e\xb7\x8a\x0c]\x077\x08\xcbl\xd2zG\"D5\xc5\xcc\xcc.\x8e\xdd*\xed\xb7}f\r_aQ\"U\x1b\xe3F\xe3\xe3=\xd05b\x9a\xdb\xaf\x14\xc8[\xa1\xcc\x89\x1b\xe10&\xfc\xa0\x9b\x1f\x81\xa7G\x1f\x04\xeb\xa39\x92\x06\x9f\x99\xd3\xbf\xd3\xeaOP\x9c\x19\xfe\x96\x87\x1e<e\xf6\xa3\x18$\x83\x86\x10\xe7T>\xa8:v$O\x81!\xc5\xe3\x0f\x02\xf8\x93\x94G \xbb\xfe\xd4\x0e\xd3h\xb9\xdd\xc4z\x84\x82\xe3STy\xdd\xdb\x9c\xd2\xf2\x07\x9b.\xb6\xbc>\xed\x85m\xef%\x11\xf2\x97\x1aBa\xf7J\x97\xe8\x8b\xb1\x10\x07\xfae\x81\xb2\xa29\xcf\xf7<\xff\x18\xfb\xc6\xf1Z\x8bY\xe2\x02\xac{\x92\xd0N\x14OYE\xf6\x0c^(_\xb0\xe8?E\xcf\xcf\xaf\x9bo\xfb\x84\xd3wZ\x95o\xac\x94\x84\x9e\xee\xbc\xc0J\x8fJ\x93\xf8D!\xe21EaPN\x10\xd8\xe35|L\x19\xb4\xde\x05\xbf\xa3\x06\x9f\xc8\xb5\xcd\xe4\x1f\xd7\x17\x06\rz\x95tU\rh\x1a\xfc\x10\x1bbd\x9dm\xe0\x95\xa0\xc3\x94\x07W\r\x14\xe6\xbd\x05\xfb\xb8\x9f\xe6\xdf\x8b\xe2\xc6\xe7~\x96\xf6S\xc5\x804P(X\xf0\x12Pq\x170\xba\xe6xc\xbc\xf4\xb2\xad\x9b+\xb2\xfe\xe19\x8c^\xba\x0b \x94\xde{\x83\xb8\xff\xe3V\x8d\xb7\x11\xe9;\x8c\xf2\xb1\xc1]\x9d\xa4\x0bL+\xd9\xb2\x18\xf5\xb5\x9fK\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Starfield Services Root Certificate Authority - G2 O=Starfield Technologies, Inc.
     * Subject: CN=Starfield Services Root Certificate Authority - G2 O=Starfield Technologies, Inc.
     * Label: "Starfield Services Root Certificate Authority - G2"
     * Serial: 0
     * MD5 Fingerprint: 17:35:74:af:7b:61:1c:eb:f4:f9:3c:e2:ee:40:f9:a2
     * SHA1 Fingerprint: 92:5a:8f:8d:2c:6d:04:e0:66:5f:59:6a:ff:22:d8:63:e8:25:6f:3f
     * SHA256 Fingerprint: 56:8d:69:05:a2:c8:87:08:a4:b3:02:51:90:ed:cf:ed:b1:97:4a:60:6a:13:c6:e5:29:0f:cb:2a:e6:3e:da:b5
     * -----BEGIN CERTIFICATE-----
     * MIID7zCCAtegAwIBAgIBADANBgkqhkiG9w0BAQsFADCBmDELMAkGA1UEBhMCVVMx
     * EDAOBgNVBAgTB0FyaXpvbmExEzARBgNVBAcTClNjb3R0c2RhbGUxJTAjBgNVBAoT
     * HFN0YXJmaWVsZCBUZWNobm9sb2dpZXMsIEluYy4xOzA5BgNVBAMTMlN0YXJmaWVs
     * ZCBTZXJ2aWNlcyBSb290IENlcnRpZmljYXRlIEF1dGhvcml0eSAtIEcyMB4XDTA5
     * MDkwMTAwMDAwMFoXDTM3MTIzMTIzNTk1OVowgZgxCzAJBgNVBAYTAlVTMRAwDgYD
     * VQQIEwdBcml6b25hMRMwEQYDVQQHEwpTY290dHNkYWxlMSUwIwYDVQQKExxTdGFy
     * ZmllbGQgVGVjaG5vbG9naWVzLCBJbmMuMTswOQYDVQQDEzJTdGFyZmllbGQgU2Vy
     * dmljZXMgUm9vdCBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkgLSBHMjCCASIwDQYJKoZI
     * hvcNAQEBBQADggEPADCCAQoCggEBANUMOsQq+U7i9b4Zl1+OiFOxHz/Lz58gE20p
     * OsgPfTz3a3Y4Y9k2YKibXlwAgLIvWX/2h/klQ4bnaRtSmpDhcePYLQ1Ob/bISdm2
     * 8xpWriu2dBTrz/sm4xq6HZYuajtYlIlHVv8loJNwU4PahHQUw2eeBGg6345AWh1K
     * Ts9DkTvnVtYAcMtS7nt9rjrnvDH5RfbCYM8TWQIrgMw0R9+53pBlbQLPLJGmpufe
     * hRhJfGZOozptqbXuNC66DQO4M99H67FrjSXZm86B0UVGMpZwh94CDklDhbZsc7tk
     * 6mFBrMnUVN+HL8cisibMn1lUaJ/8viovxFUcdUBgF4UCVTmLfwUCAwEAAaNCMEAw
     * DwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFJxfAN+q
     * AdcwKziIorhtSpzyEZGDMA0GCSqGSIb3DQEBCwUAA4IBAQBLNqaEd2ndOxmfZyMI
     * bw5hyf2E3F/YNoHN2BtBLZ9g3ccaaNnRbobhiCPPE95Dz+I0swSdHynVv/heyNXB
     * ve6SbzJ08pGCL72CQnqtKrcgfU28elUSwhXqvfdqlS5sdJ/PHLTyxQGjhdByPq1z
     * qwubdQxtRbeOlKyWN7Wg0I8VRw7j6IPdj/3vQQF3zCepYoUz8jcI73HPdwbeyBkd
     * iEDPfUYd/x7H4c7/I9vG+o1VTqkC50cRRj70/b17KSa7qWFiNyi2LSr2EIZkyXCn
     * 0q23KXB56jzaYyWf/Wi3MOxw+3WKt21gZ7IeyLnp2KhvAotnDU0mV3HaIPzBSlCN
     * sSi6
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x100\x0e\x06\x03U\x04\x08\x13\x07Arizona1\x130\x11\x06\x03U\x04\x07\x13\nScottsdale1%0#\x06\x03U\x04\n\x13\x1cStarfield Technologies, Inc.1;09\x06\x03U\x04\x03\x132Starfield Services Root Certificate Authority - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd5\x0c:\xc4*\xf9N\xe2\xf5\xbe\x19\x97_\x8e\x88S\xb1\x1f?\xcb\xcf\x9f \x13m):\xc8\x0f}<\xf7kv8c\xd96`\xa8\x9b^\\\x00\x80\xb2/Y\x7f\xf6\x87\xf9%C\x86\xe7i\x1bR\x9a\x90\xe1q\xe3\xd8-\rNo\xf6\xc8I\xd9\xb6\xf3\x1aV\xae+\xb6t\x14\xeb\xcf\xfb&\xe3\x1a\xba\x1d\x96.j;X\x94\x89GV\xff%\xa0\x93pS\x83\xda\x84t\x14\xc3g\x9e\x04h:\xdf\x8e@Z\x1dJN\xcfC\x91;\xe7V\xd6\x00p\xcbR\xee{}\xae:\xe7\xbc1\xf9E\xf6\xc2`\xcf\x13Y\x02+\x80\xcc4G\xdf\xb9\xde\x90em\x02\xcf,\x91\xa6\xa6\xe7\xde\x85\x18I|fN\xa3:m\xa9\xb5\xee4.\xba\r\x03\xb83\xdfG\xeb\xb1k\x8d%\xd9\x9b\xce\x81\xd1EF2\x96p\x87\xde\x02\x0eIC\x85\xb6ls\xbbd\xeaaA\xac\xc9\xd4T\xdf\x87/\xc7\"\xb2&\xcc\x9fYTh\x9f\xfc\xbe*/\xc4U\x1cu@`\x17\x85\x02U9\x8b\x7f\x05\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TWCA Global Root CA O=TAIWAN-CA OU=Root CA
     * Subject: CN=TWCA Global Root CA O=TAIWAN-CA OU=Root CA
     * Label: "TWCA Global Root CA"
     * Serial: 3262
     * MD5 Fingerprint: f9:03:7e:cf:e6:9e:3c:73:7a:2a:90:07:69:ff:2b:96
     * SHA1 Fingerprint: 9c:bb:48:53:f6:a4:f6:d3:52:a4:e8:32:52:55:60:13:f5:ad:af:65
     * SHA256 Fingerprint: 59:76:90:07:f7:68:5d:0f:cd:50:87:2f:9f:95:d5:75:5a:5b:2b:45:7d:81:f3:69:2b:61:0a:98:67:2f:0e:1b
     * -----BEGIN CERTIFICATE-----
     * MIIFQTCCAymgAwIBAgICDL4wDQYJKoZIhvcNAQELBQAwUTELMAkGA1UEBhMCVFcx
     * EjAQBgNVBAoTCVRBSVdBTi1DQTEQMA4GA1UECxMHUm9vdCBDQTEcMBoGA1UEAxMT
     * VFdDQSBHbG9iYWwgUm9vdCBDQTAeFw0xMjA2MjcwNjI4MzNaFw0zMDEyMzExNTU5
     * NTlaMFExCzAJBgNVBAYTAlRXMRIwEAYDVQQKEwlUQUlXQU4tQ0ExEDAOBgNVBAsT
     * B1Jvb3QgQ0ExHDAaBgNVBAMTE1RXQ0EgR2xvYmFsIFJvb3QgQ0EwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQCwBdvI64zEbooh745NnHEKH1Jw7W2CnJfF
     * 10xORUnLQEK1EjRsGcJ0pDFfhQKX7EMzClPSnIyOt7h52yvVavKOZsTuKwEHktSz
     * 0ALfUPZVr2YOy+BHYC8rMjk1Ujoog/h7FsYYuGLWRyWRzvAZEk2tY/XTP3VfKfCh
     * MBwqoJimFb3u/Rk28OKRQ4/6ytYQJ0lM793B8YVwm8rqqFpD/G2Gb3PpN0Wp8DbH
     * zIh1HrtsBv+baz4X7GGqcXzGHaL3SekVtTzWoWH1EfcFbx39Eb7QMAfCKbAJTibc
     * 46KokWofwpFFiFzlmLhxpRUZyXx1EcxwdE8tmx2RRP1WKKD+u4ZqyPpcC1jcxkt2
     * yKsi2XMPpfRaAok/T54igu6idFMqPVMnaR1sjjIsZAAmY2E2TqNGtz99sy2sbZCi
     * laLOz9qC5wc0GZbpuCGqKX6mOL6OKUohZnkfs8O1CWfe1tQHRvMq2uYiN2DLgbYP
     * oA/pyJV/v1WRBXrPPRXAb94JlAGD1zQbzECl8LibZ9WYkTunhHiVJqRaCPgrdLQA
     * BDzfuBSO6N+pjWxnkjMdwLfS7JLIvgm/LCkFbwJrnu+8vyq8W8BQj0FwcYeyTbcE
     * qYSjMq+u7msXi7Kx/mzhkIyIqJdIzshNy/MGz19qCkKxHh53L46g5pIOBvwFItIm
     * 4TFRfTLcDwIDAQABoyMwITAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB
     * /zANBgkqhkiG9w0BAQsFAAOCAgEAXzSBdu+WHdXltdkCY4QWwa6gcFGn90xHNcgL
     * 1yg9iXHZqjNB6hQbbCEAwGxCGX6faVsgQt+i0trEfJdLjbDorMjupWkEmQqSpqsn
     * LhpNgb+E1HAerUf+/UqdM+DyucRFCCEK2mlpc3INvjT+lIutwx4116KD7+U4x6WF
     * H6vPNOw/KP4M8VeGTslV9xzU2KV9Bnpv1d8Q34FOIWWxtuEXeZVFBs5fzNxGiWNo
     * RI2T9GRwoD2dKAXDOXC4Ynsg/eTb6QihuJ49CcdP+yz4k3ZB3lLg4VfSnQO8d57+
     * nile98FRYB/e2guyLXW3Q0iT5/Z5xoRdgFlglPx4mI88k1HtQJAH32RjJMtOcQWh
     * 15QaiDLxInQirqWm2BJpTGCjAu4r7NRjkgtevi92a6O2JryPA9gK8kxkRr05YuWW
     * 6zRjESjMlfGt7+/cgFhI6Uu46mWs6fyAtbXIRfmswZ/ZuepiiI7E8UuDEq3mi4TW
     * nsLrgxifarsbJGAzcMzs9zLzXNl5fe+epP7JI8Mk7hWSsT2RTyaGvWZzJBPqpK5j
     * wa19hAM8EHiGG3njxPPyBJUgriOCxLM6AGK/5jYk4Ve6xx6QddVfP5VhK8E7zeWz
     * aGHQRiapIVJpLesux+t3zqY6tQMzT3bR51xUAV3LePTJDL/PEo4XLSNolOer/qmy
     * KwbQBM0=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02TW1\x120\x10\x06\x03U\x04\n\x13\tTAIWAN-CA1\x100\x0e\x06\x03U\x04\x0b\x13\x07Root CA1\x1c0\x1a\x06\x03U\x04\x03\x13\x13TWCA Global Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb0\x05\xdb\xc8\xeb\x8c\xc4n\x8a!\xef\x8eM\x9cq\n\x1fRp\xedm\x82\x9c\x97\xc5\xd7LNEI\xcb@B\xb5\x124l\x19\xc2t\xa41_\x85\x02\x97\xecC3\nS\xd2\x9c\x8c\x8e\xb7\xb8y\xdb+\xd5j\xf2\x8ef\xc4\xee+\x01\x07\x92\xd4\xb3\xd0\x02\xdfP\xf6U\xaff\x0e\xcb\xe0G`/+295R:(\x83\xf8{\x16\xc6\x18\xb8b\xd6G%\x91\xce\xf0\x19\x12M\xadc\xf5\xd3?u_)\xf0\xa10\x1c*\xa0\x98\xa6\x15\xbd\xee\xfd\x196\xf0\xe2\x91C\x8f\xfa\xca\xd6\x10\'IL\xef\xdd\xc1\xf1\x85p\x9b\xca\xea\xa8ZC\xfcm\x86os\xe97E\xa9\xf06\xc7\xcc\x88u\x1e\xbbl\x06\xff\x9bk>\x17\xeca\xaaq|\xc6\x1d\xa2\xf7I\xe9\x15\xb5<\xd6\xa1a\xf5\x11\xf7\x05o\x1d\xfd\x11\xbe\xd00\x07\xc2)\xb0\tN&\xdc\xe3\xa2\xa8\x91j\x1f\xc2\x91E\x88\\\xe5\x98\xb8q\xa5\x15\x19\xc9|u\x11\xccptO-\x9b\x1d\x91D\xfdV(\xa0\xfe\xbb\x86j\xc8\xfa\\\x0bX\xdc\xc6Kv\xc8\xab\"\xd9s\x0f\xa5\xf4Z\x02\x89?O\x9e\"\x82\xee\xa2tS*=S\'i\x1dl\x8e2,d\x00&ca6N\xa3F\xb7?}\xb3-\xacm\x90\xa2\x95\xa2\xce\xcf\xda\x82\xe7\x074\x19\x96\xe9\xb8!\xaa)~\xa68\xbe\x8e)J!fy\x1f\xb3\xc3\xb5\tg\xde\xd6\xd4\x07F\xf3*\xda\xe6\"7`\xcb\x81\xb6\x0f\xa0\x0f\xe9\xc8\x95\x7f\xbfU\x91\x05z\xcf=\x15\xc0o\xde\t\x94\x01\x83\xd74\x1b\xcc@\xa5\xf0\xb8\x9bg\xd5\x98\x91;\xa7\x84x\x95&\xa4Z\x08\xf8+t\xb4\x00\x04<\xdf\xb8\x14\x8e\xe8\xdf\xa9\x8dlg\x923\x1d\xc0\xb7\xd2\xec\x92\xc8\xbe\t\xbf,)\x05o\x02k\x9e\xef\xbc\xbf*\xbc[\xc0P\x8fApq\x87\xb2M\xb7\x04\xa9\x84\xa32\xaf\xae\xeek\x17\x8b\xb2\xb1\xfel\xe1\x90\x8c\x88\xa8\x97H\xce\xc8M\xcb\xf3\x06\xcf_j\nB\xb1\x1e\x1ew/\x8e\xa0\xe6\x92\x0e\x06\xfc\x05\"\xd2&\xe11Q}2\xdc\x0f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Hongkong Post Root CA 3 O=Hongkong Post
     * Subject: CN=Hongkong Post Root CA 3 O=Hongkong Post
     * Label: "Hongkong Post Root CA 3"
     * Serial: 46170865288971385588281144162979347873371282084
     * MD5 Fingerprint: 11:fc:9f:bd:73:30:02:8a:fd:3f:f3:58:b9:cb:20:f0
     * SHA1 Fingerprint: 58:a2:d0:ec:20:52:81:5b:c1:f3:f8:64:02:24:4e:c2:8e:02:4b:02
     * SHA256 Fingerprint: 5a:2f:c0:3f:0c:83:b0:90:bb:fa:40:60:4b:09:88:44:6c:76:36:18:3d:f9:84:6e:17:10:1a:44:7f:b8:ef:d6
     * -----BEGIN CERTIFICATE-----
     * MIIFzzCCA7egAwIBAgIUCBZfikyl7ADJk0DfxMauI7gcWqQwDQYJKoZIhvcNAQEL
     * BQAwbzELMAkGA1UEBhMCSEsxEjAQBgNVBAgTCUhvbmcgS29uZzESMBAGA1UEBxMJ
     * SG9uZyBLb25nMRYwFAYDVQQKEw1Ib25na29uZyBQb3N0MSAwHgYDVQQDExdIb25n
     * a29uZyBQb3N0IFJvb3QgQ0EgMzAeFw0xNzA2MDMwMjI5NDZaFw00MjA2MDMwMjI5
     * NDZaMG8xCzAJBgNVBAYTAkhLMRIwEAYDVQQIEwlIb25nIEtvbmcxEjAQBgNVBAcT
     * CUhvbmcgS29uZzEWMBQGA1UEChMNSG9uZ2tvbmcgUG9zdDEgMB4GA1UEAxMXSG9u
     * Z2tvbmcgUG9zdCBSb290IENBIDMwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIK
     * AoICAQCziNfqzg8gTr7m1gNt7ln8wlffKWihgw4+aMdoWJwcYEuJQwy51BWy7sFO
     * dem1p+/l6TWZ5Mwc50tfjTMwIDNT2aa71T4Tjukfh0mtUC1Qyhi+AViiE3CWu4mI
     * VoBc+L0sPOFMV4i707mV78vH9toxdCim5lSJ9UExyuUmGs2C4HDaOym71QP1mbpV
     * 9WTRYA6ziUm4ii8F0oRFKHyPaFASePwLtVPLwpgchKOesL4jpNrcyCse2m5FHomY
     * 2vkALgbpDDtw1VAliJnLzXNg99X/NWfFobxeq81KuEXryGgeDQ0URhLj0mRiikKY
     * vLTGCAj4/ahMZJx2Ab0vqWwzD9g/KLg8aQFChn5pwckGyuV6RmXpwtZQQS4/t+Tt
     * bNe/JgERohYpSms0BpDsE9K2+2p20jzt8NYt3eEV7KObLyzJPivkaTv/ciWxNoZb
     * x39ri1UbSsUgYT2uy1DhCDq+sI9jQVMwCFk8mB13umOResoQUGC/8Ne8lYePl8X+
     * l2oBlKN8W4UdKjk60FSh0Tlxnf0h+bV78OLgAo9uliQlLKAeLKjEiafv7ZkGL7YK
     * TE/bosw3Gq9HhS2KX8Q0NEwA/RiTZxPRN+ZItIsGxVd7GYYKecsAyVKvQv83j+Gj
     * Hno9UKtjBucVtT+2RTeUN7F+8kjDf8V1/peNRY8apxpyKBpADwIDAQABo2MwYTAP
     * BgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAfBgNVHSMEGDAWgBQXnc0e
     * i9Y5K3DTXNSguB+wAPzFYTAdBgNVHQ4EFgQUF53NHovWOStw01zUoLgfsAD8xWEw
     * DQYJKoZIhvcNAQELBQADggIBAFbVe27mIgHSQpsY1Q7XZiNc4/6gx5LS6ZStS6LG
     * 7BJ8dNVI0lkUmcDrudHr9EgwW62nV3OZqdPlt9EuWSRY3GguLmLYauRwCy0gUCCk
     * MpXRAJi70/33MvJJrsZ64Ee+bs7Lo3I6LWldy8joRTnU+kLBEUx3XZL7av9YROXr
     * gZ6voJmtvqkBZss4HTzfQx/0TW60uhdG/H39h4F5ag0zD/ov+BS5gLNdTaqX4fnk
     * GMX41TiMJjz98iji7lpJiCzfeT2OnpA8vUFKOt1b9pq0zj8lMH8yfaIDlNDceqFS
     * 3m6TjRgm/VWsvY+b0s+v54Ysyx8Jb6NvqYTUc79NoXQbTiNg8swOqn+knEwlqLJm
     * Ozj/2ZQw9nKEvmhVEA/GcywWaZMH/rFF7buiVWqw2rVKAiUnhde3t4ZEFolsgCs+
     * l6mc1X5VTMbeRRAc6uk7nwNT7u56AQIWeNTowr5GdogTPyK7SBIdUgC0An4hGh6c
     * JfTzPV4e0hz5sy229zdcxsshTrD3mUcYhcErulWuBurQB7Lcq9CClnXO0lD+mefP
     * L5/ndtFhKvshuzHQqp9HpLIiyhY6UFfEW0NnxWViA0kB60PZ2Pierc+xYw5F9KBa
     * LJstxabArahH9CdMOA0uG0k7UvToiIMrVCjU8jVStDKDYmlkDJGcn5fqdBb9HxEG
     * mpv0
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02HK1\x120\x10\x06\x03U\x04\x08\x13\tHong Kong1\x120\x10\x06\x03U\x04\x07\x13\tHong Kong1\x160\x14\x06\x03U\x04\n\x13\rHongkong Post1 0\x1e\x06\x03U\x04\x03\x13\x17Hongkong Post Root CA 3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb3\x88\xd7\xea\xce\x0f N\xbe\xe6\xd6\x03m\xeeY\xfc\xc2W\xdf)h\xa1\x83\x0e>h\xc7hX\x9c\x1c`K\x89C\x0c\xb9\xd4\x15\xb2\xee\xc1Nu\xe9\xb5\xa7\xef\xe5\xe95\x99\xe4\xcc\x1c\xe7K_\x8d30 3S\xd9\xa6\xbb\xd5>\x13\x8e\xe9\x1f\x87I\xadP-P\xca\x18\xbe\x01X\xa2\x13p\x96\xbb\x89\x88V\x80\\\xf8\xbd,<\xe1LW\x88\xbb\xd3\xb9\x95\xef\xcb\xc7\xf6\xda1t(\xa6\xe6T\x89\xf5A1\xca\xe5&\x1a\xcd\x82\xe0p\xda;)\xbb\xd5\x03\xf5\x99\xbaU\xf5d\xd1`\x0e\xb3\x89I\xb8\x8a/\x05\xd2\x84E(|\x8fhP\x12x\xfc\x0b\xb5S\xcb\xc2\x98\x1c\x84\xa3\x9e\xb0\xbe#\xa4\xda\xdc\xc8+\x1e\xdanE\x1e\x89\x98\xda\xf9\x00.\x06\xe9\x0c;p\xd5P%\x88\x99\xcb\xcds`\xf7\xd5\xff5g\xc5\xa1\xbc^\xab\xcdJ\xb8E\xeb\xc8h\x1e\r\r\x14F\x12\xe3\xd2db\x8aB\x98\xbc\xb4\xc6\x08\x08\xf8\xfd\xa8Ld\x9cv\x01\xbd/\xa9l3\x0f\xd8?(\xb8<i\x01B\x86~i\xc1\xc9\x06\xca\xe5zFe\xe9\xc2\xd6PA.?\xb7\xe4\xedl\xd7\xbf&\x01\x11\xa2\x16)Jk4\x06\x90\xec\x13\xd2\xb6\xfbjv\xd2<\xed\xf0\xd6-\xdd\xe1\x15\xec\xa3\x9b/,\xc9>+\xe4i;\xffr%\xb16\x86[\xc7\x7fk\x8bU\x1bJ\xc5 a=\xae\xcbP\xe1\x08:\xbe\xb0\x8fcAS0\x08Y<\x98\x1dw\xbac\x91z\xca\x10P`\xbf\xf0\xd7\xbc\x95\x87\x8f\x97\xc5\xfe\x97j\x01\x94\xa3|[\x85\x1d*9:\xd0T\xa1\xd19q\x9d\xfd!\xf9\xb5{\xf0\xe2\xe0\x02\x8fn\x96$%,\xa0\x1e,\xa8\xc4\x89\xa7\xef\xed\x99\x06/\xb6\nLO\xdb\xa2\xcc7\x1a\xafG\x85-\x8a_\xc444L\x00\xfd\x18\x93g\x13\xd17\xe6H\xb4\x8b\x06\xc5W{\x19\x86\ny\xcb\x00\xc9R\xafB\xff7\x8f\xe1\xa3\x1ez=P\xabc\x06\xe7\x15\xb5?\xb6E7\x947\xb1~\xf2H\xc3\x7f\xc5u\xfe\x97\x8dE\x8f\x1a\xa7\x1ar(\x1a@\x0f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TrustCor ECA-1 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Subject: CN=TrustCor ECA-1 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Label: "TrustCor ECA-1"
     * Serial: 9548242946988625984
     * MD5 Fingerprint: 27:92:23:1d:0a:f5:40:7c:e9:e6:6b:9d:d8:f5:e7:6c
     * SHA1 Fingerprint: 58:d1:df:95:95:67:6b:63:c0:f0:5b:1c:17:4d:8b:84:0b:c8:78:bd
     * SHA256 Fingerprint: 5a:88:5d:b1:9c:01:d9:12:c5:75:93:88:93:8c:af:bb:df:03:1a:b2:d4:8e:91:ee:15:58:9b:42:97:1d:03:9c
     * -----BEGIN CERTIFICATE-----
     * MIIEIDCCAwigAwIBAgIJAISCLF8cYtBAMA0GCSqGSIb3DQEBCwUAMIGcMQswCQYD
     * VQQGEwJQQTEPMA0GA1UECAwGUGFuYW1hMRQwEgYDVQQHDAtQYW5hbWEgQ2l0eTEk
     * MCIGA1UECgwbVHJ1c3RDb3IgU3lzdGVtcyBTLiBkZSBSLkwuMScwJQYDVQQLDB5U
     * cnVzdENvciBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkxFzAVBgNVBAMMDlRydXN0Q29y
     * IEVDQS0xMB4XDTE2MDIwNDEyMzIzM1oXDTI5MTIzMTE3MjgwN1owgZwxCzAJBgNV
     * BAYTAlBBMQ8wDQYDVQQIDAZQYW5hbWExFDASBgNVBAcMC1BhbmFtYSBDaXR5MSQw
     * IgYDVQQKDBtUcnVzdENvciBTeXN0ZW1zIFMuIGRlIFIuTC4xJzAlBgNVBAsMHlRy
     * dXN0Q29yIENlcnRpZmljYXRlIEF1dGhvcml0eTEXMBUGA1UEAwwOVHJ1c3RDb3Ig
     * RUNBLTEwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDPj+ARtZ+odnbb
     * 3w9U73NjKYKtR8aja+3+XzP4Q1HpGjORMRegdMTUpwHmspI+ap3tDvl0mEDTPwOA
     * BoJA6LHip1GnHYMma6ve+heRK9jGrB6xnhkB1Zem6g23xFUfJ3zSCNV2HykVh0A5
     * 3ThFEXXQmqc04L/NyFIduUd+Dbi7xgz2c1cWWn5DkR9VOsZtRASqnKmcp0yJF4Ou
     * owReUoCLHhIlERnXDH19MURB6tuvsBzvgdAsxZohmz3tQjtQJvLsznFhBmIhVE5/
     * wZ0+fyCMgMsq2JdiyIMzkX2woloPV+g7zPIlstR8L+xNxqE6FXrntl019fZISjZF
     * ZtS6mFjBAgMBAAGjYzBhMB0GA1UdDgQWBBREnkj1zG1I1KBLf/5ZJC+Dl5mahjAf
     * BgNVHSMEGDAWgBREnkj1zG1I1KBLf/5ZJC+Dl5mahjAPBgNVHRMBAf8EBTADAQH/
     * MA4GA1UdDwEB/wQEAwIBhjANBgkqhkiG9w0BAQsFAAOCAQEABT41XBVwm8nHc2Fv
     * civUwo/yQ10CzsSUuZQRg2dd4mdsdXa/uwyqNsatR5Nj3B5+1t4u/ukZMjgDfxT2
     * AHMsWbEhBuH7rBiVDKP/mZb3Kyeb1STMHd3BOuCYRLDE5D53sXOpZCz2HAF8P11F
     * hcCF5yWPldwX8zyfGm6wyuMdKulMY/okYWLW2n62HGz1Ah3UKt1VkOsqEUc8Ll50
     * soIipX1TH0XsJ5F95yIW6MBoNtjG8U+ARDL54dHRHareqKucBK+tIA5kmE2la8BI
     * WJZpTdwHjFGTot+fDz2LYLSCjaoITmJF4PkL0uDgPFveXHEnJcLmA4GLEFPjx1Wi
     * tJ/X5g==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PA1\x0f0\r\x06\x03U\x04\x08\x0c\x06Panama1\x140\x12\x06\x03U\x04\x07\x0c\x0bPanama City1$0\"\x06\x03U\x04\n\x0c\x1bTrustCor Systems S. de R.L.1\'0%\x06\x03U\x04\x0b\x0c\x1eTrustCor Certificate Authority1\x170\x15\x06\x03U\x04\x03\x0c\x0eTrustCor ECA-1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xcf\x8f\xe0\x11\xb5\x9f\xa8vv\xdb\xdf\x0fT\xefsc)\x82\xadG\xc6\xa3k\xed\xfe_3\xf8CQ\xe9\x1a3\x911\x17\xa0t\xc4\xd4\xa7\x01\xe6\xb2\x92>j\x9d\xed\x0e\xf9t\x98@\xd3?\x03\x80\x06\x82@\xe8\xb1\xe2\xa7Q\xa7\x1d\x83&k\xab\xde\xfa\x17\x91+\xd8\xc6\xac\x1e\xb1\x9e\x19\x01\xd5\x97\xa6\xea\r\xb7\xc4U\x1f\'|\xd2\x08\xd5v\x1f)\x15\x87@9\xdd8E\x11u\xd0\x9a\xa74\xe0\xbf\xcd\xc8R\x1d\xb9G~\r\xb8\xbb\xc6\x0c\xf6sW\x16Z~C\x91\x1fU:\xc6mD\x04\xaa\x9c\xa9\x9c\xa7L\x89\x17\x83\xae\xa3\x04^R\x80\x8b\x1e\x12%\x11\x19\xd7\x0c}}1DA\xea\xdb\xaf\xb0\x1c\xef\x81\xd0,\xc5\x9a!\x9b=\xedB;P&\xf2\xec\xceqa\x06b!TN\x7f\xc1\x9d>\x7f \x8c\x80\xcb*\xd8\x97b\xc8\x833\x91}\xb0\xa2Z\x0fW\xe8;\xcc\xf2%\xb2\xd4|/\xecM\xc6\xa1:\x15z\xe7\xb6]5\xf5\xf6HJ6Ef\xd4\xba\x98X\xc1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Certum Trusted Network CA O=Unizeto Technologies S.A. OU=Certum Certification Authority
     * Subject: CN=Certum Trusted Network CA O=Unizeto Technologies S.A. OU=Certum Certification Authority
     * Label: "Certum Trusted Network CA"
     * Serial: 279744
     * MD5 Fingerprint: d5:e9:81:40:c5:18:69:fc:46:2c:89:75:62:0f:aa:78
     * SHA1 Fingerprint: 07:e0:32:e0:20:b7:2c:3f:19:2f:06:28:a2:59:3a:19:a7:0f:06:9e
     * SHA256 Fingerprint: 5c:58:46:8d:55:f5:8e:49:7e:74:39:82:d2:b5:00:10:b6:d1:65:37:4a:cf:83:a7:d4:a3:2d:b7:68:c4:40:8e
     * -----BEGIN CERTIFICATE-----
     * MIIDuzCCAqOgAwIBAgIDBETAMA0GCSqGSIb3DQEBBQUAMH4xCzAJBgNVBAYTAlBM
     * MSIwIAYDVQQKExlVbml6ZXRvIFRlY2hub2xvZ2llcyBTLkEuMScwJQYDVQQLEx5D
     * ZXJ0dW0gQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkxIjAgBgNVBAMTGUNlcnR1bSBU
     * cnVzdGVkIE5ldHdvcmsgQ0EwHhcNMDgxMDIyMTIwNzM3WhcNMjkxMjMxMTIwNzM3
     * WjB+MQswCQYDVQQGEwJQTDEiMCAGA1UEChMZVW5pemV0byBUZWNobm9sb2dpZXMg
     * Uy5BLjEnMCUGA1UECxMeQ2VydHVtIENlcnRpZmljYXRpb24gQXV0aG9yaXR5MSIw
     * IAYDVQQDExlDZXJ0dW0gVHJ1c3RlZCBOZXR3b3JrIENBMIIBIjANBgkqhkiG9w0B
     * AQEFAAOCAQ8AMIIBCgKCAQEA4/t9o3K6wvDJFIf1awFO4W5AB7ptJ11/91sts1rH
     * UV+rpDKmYYe2bg+G0jACl/jXaVehGDldamR5xgFZrDwxSjh80gTSSyjoIF87B6LM
     * TXPb865Px1bVWqeWifrzq2jUI4ZZJ88JJ7ysbnKDHDBy3+Ci6dLhdHUZvSqeexVU
     * BBvXQzmtVSjF4hq79MDkrjhJM8x2hZ85RdKknvISjFH4fOQtf/WsX+sWn7Et0brM
     * kUJ3TCXJkDhv2/DM+44el1k+1WBO5gUo7Ul5E0u6SNsv+XLTOcr+H9g0cvW0QM8x
     * AcPs3hEtF10fuFDRXhmnad4HMyjKUJX5p1TLVIZQRan5SQIDAQABo0IwQDAPBgNV
     * HRMBAf8EBTADAQH/MB0GA1UdDgQWBBQIds3LB/8k9sXN7buQvOKEN0Z19zAOBgNV
     * HQ8BAf8EBAMCAQYwDQYJKoZIhvcNAQEFBQADggEBAKaorSLOAT2mo/9i0Eidi15y
     * sHhE49wcrwn9I0j6vSrEuVUEtRCjjSfeC4Jj0O7eDDd5QVsisrCaQVymcODU0HfL
     * I9MA4GxWL+FpDQ3Zqr8hgVDZBqWo/5U30Kr+4rP1mS1FhIrlQgnXdAIv94nYmem8
     * J9RHjboNRhx3zxSkHLmkMcScKHQDNP8zGSal6Q10tz6XxnboJ5ajZt3hrvJBW8qY
     * VoNzcOSGGtIxQbovvi0TWnZvTuhOgQ4/WwMioBK+ZlgRSssDxLQqKi2WF+A5VLxI
     * 03YnnZotBqbJ7DnSq9ufmgsnAjUpsUCV5/nonFWIGUbWtzT1fs45mtk48VH3Tyw=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PL1\"0 \x06\x03U\x04\n\x13\x19Unizeto Technologies S.A.1\'0%\x06\x03U\x04\x0b\x13\x1eCertum Certification Authority1\"0 \x06\x03U\x04\x03\x13\x19Certum Trusted Network CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xe3\xfb}\xa3r\xba\xc2\xf0\xc9\x14\x87\xf5k\x01N\xe1n@\x07\xbam\']\x7f\xf7[-\xb3Z\xc7Q_\xab\xa42\xa6a\x87\xb6n\x0f\x86\xd20\x02\x97\xf8\xd7iW\xa1\x189]jdy\xc6\x01Y\xac<1J8|\xd2\x04\xd2K(\xe8 _;\x07\xa2\xccMs\xdb\xf3\xaeO\xc7V\xd5Z\xa7\x96\x89\xfa\xf3\xabh\xd4#\x86Y\'\xcf\t\'\xbc\xacnr\x83\x1c0r\xdf\xe0\xa2\xe9\xd2\xe1tu\x19\xbd*\x9e{\x15T\x04\x1b\xd7C9\xadU(\xc5\xe2\x1a\xbb\xf4\xc0\xe4\xae8I3\xccv\x85\x9f9E\xd2\xa4\x9e\xf2\x12\x8cQ\xf8|\xe4-\x7f\xf5\xac_\xeb\x16\x9f\xb1-\xd1\xba\xcc\x91BwL%\xc9\x908o\xdb\xf0\xcc\xfb\x8e\x1e\x97Y>\xd5`N\xe6\x05(\xedIy\x13K\xbaH\xdb/\xf9r\xd39\xca\xfe\x1f\xd84r\xf5\xb4@\xcf1\x01\xc3\xec\xde\x11-\x17]\x1f\xb8P\xd1^\x19\xa7i\xde\x073(\xcaP\x95\xf9\xa7T\xcbT\x86PE\xa9\xf9I\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=CFCA EV ROOT O=China Financial Certification Authority
     * Subject: CN=CFCA EV ROOT O=China Financial Certification Authority
     * Label: "CFCA EV ROOT"
     * Serial: 407555286
     * MD5 Fingerprint: 74:e1:b6:ed:26:7a:7a:44:30:33:94:ab:7b:27:81:30
     * SHA1 Fingerprint: e2:b8:29:4b:55:84:ab:6b:58:c2:90:46:6c:ac:3f:b8:39:8f:84:83
     * SHA256 Fingerprint: 5c:c3:d7:8e:4e:1d:5e:45:54:7a:04:e6:87:3e:64:f9:0c:f9:53:6d:1c:cc:2e:f8:00:f3:55:c4:c5:fd:70:fd
     * -----BEGIN CERTIFICATE-----
     * MIIFjTCCA3WgAwIBAgIEGErM1jANBgkqhkiG9w0BAQsFADBWMQswCQYDVQQGEwJD
     * TjEwMC4GA1UECgwnQ2hpbmEgRmluYW5jaWFsIENlcnRpZmljYXRpb24gQXV0aG9y
     * aXR5MRUwEwYDVQQDDAxDRkNBIEVWIFJPT1QwHhcNMTIwODA4MDMwNzAxWhcNMjkx
     * MjMxMDMwNzAxWjBWMQswCQYDVQQGEwJDTjEwMC4GA1UECgwnQ2hpbmEgRmluYW5j
     * aWFsIENlcnRpZmljYXRpb24gQXV0aG9yaXR5MRUwEwYDVQQDDAxDRkNBIEVWIFJP
     * T1QwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQDXXWvNED8fBVnVBU03
     * sQ7smCuOFR36k0sXgiFxEFLXUWRwFsJVaU2OFW2fvwwbwuCjZ9YMrM8irq93VCpL
     * TIpTUnrD7i7es3ElweldPe6hL6P3KjzJIx1qqx2hp/Hz7KDVRM8Vz3IvHWOX6Jn5
     * /ZOkVIBMUtRSqy5J35DNuF++P96hyk0g1CXohClTt7GIH//62pCfCqktQT+x8Rgp
     * 7hZZLDRJGqgG16iI0gNyejLi6mhNbiyWZXvKWfry4t3uMCz7zEasxGPrb382KzRz
     * EpR/38wmnvFyXVBlWY9ps4deMm/DGIq1lY+wejfeWkU7xzbh72fROdOXW3NiGUgt
     * hxwG+3SYIElz8AXSG7Ggo7cbcNOIabla1jj0Ytwli3i/+Oh+uFzJlU9fpy25IGvP
     * a931DfSCt/SyZi4QKPaXWnuWFo8BGS1sbn85WAZkgwGDg8NNkt0yxoekN+kWzqot
     * aK8KgWU6cMGbrU1tVMoqLUuFG7OA5nBFDWteNfB/O7ic5ARwiRIlk9oKmSJgamNg
     * TnYGmE69g60dWIolhdLHZR4tjsbftsbhf4oEIRUpdPA+nJCdDC7xij5aqgwJHsfV
     * PKPtl8MeNPo4+QgO48BdK4PRVmrJtqhUUy54Mmc9gn900PvhtgVguXDbjgv5E1hv
     * cWAQUhC5wUEJ73IfZzF4/5YFjQIDAQABo2MwYTAfBgNVHSMEGDAWgBTj/i39KNAL
     * tbq2osS/BqoFjJP7LzAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAd
     * BgNVHQ4EFgQU4/4t/SjQC7W6tqLEvwaqBYyT+y8wDQYJKoZIhvcNAQELBQADggIB
     * ACXGumvrh8vegjmWPfBEp2uEcwPenStPuiB/vHiyz5ewG5zz13ku9Ui20vsXiObT
     * ej/tUxPQ4i9qecsAIyjmHjdXNYmEwnZPNDatZ8POQQaIxffu2Bq41gt/UP+TqhdL
     * jOztUmCypAbqTuv0axn96/Ua4CUqmtzHQTb3yHQFhDmVOdYLO6Qn+gjYXB74BGBS
     * ESgoA//vU2YApUo0FmZ8/Qmkrp5nGm9BC2sGE5uPhnEFtC+NiWYzKXZUmhH4J/qy
     * P5Hgzg0b8zAarb8iXRvTvyUFTeGSGn+ZnzxEk8rUQElsgIfXBDrDMlI1Dlb4pd19
     * xIsNER9Tyx6yF7Zod1rg1MvIB671Oi6ON7fQAUtDKXeMOZePglr4UeWJoBjnaH9d
     * Ci77o0cOPaYjesYBx4/IXr9tgFa+iiS6M+qf4TIRnvHST4D2G0CvOJ4RUHlzEhLN
     * 5mydLIhyPDCBBpEi6lmt2hkuIsKNuYyH4Ga8cyNfIWRjgEj1oDwYPZTISEEdQLpe
     * /v5WOaHIz16eGWRGENoXkbcFgKyLmZJ956LYBws2J+dIeWCKw9cTXPhyQN9Ky8+Z
     * AAoACxGV2lZFA4gKn2fQ1XmxqI1AbQ3CekD6819kR5LLU7m7Wc5P/dAVUwHY3+vZ
     * 5nbv0CO7O6l5s9UCKc2Jo5YPSjXnTkLAdc0Hz+Ys63su
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CN100.\x06\x03U\x04\n\x0c\'China Financial Certification Authority1\x150\x13\x06\x03U\x04\x03\x0c\x0cCFCA EV ROOT",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xd7]k\xcd\x10?\x1f\x05Y\xd5\x05M7\xb1\x0e\xec\x98+\x8e\x15\x1d\xfa\x93K\x17\x82!q\x10R\xd7Qdp\x16\xc2UiM\x8e\x15m\x9f\xbf\x0c\x1b\xc2\xe0\xa3g\xd6\x0c\xac\xcf\"\xae\xafwT*KL\x8aSRz\xc3\xee.\xde\xb3q%\xc1\xe9]=\xee\xa1/\xa3\xf7*<\xc9#\x1dj\xab\x1d\xa1\xa7\xf1\xf3\xec\xa0\xd5D\xcf\x15\xcfr/\x1dc\x97\xe8\x99\xf9\xfd\x93\xa4T\x80LR\xd4R\xab.I\xdf\x90\xcd\xb8_\xbe?\xde\xa1\xcaM \xd4%\xe8\x84)S\xb7\xb1\x88\x1f\xff\xfa\xda\x90\x9f\n\xa9-A?\xb1\xf1\x18)\xee\x16Y,4I\x1a\xa8\x06\xd7\xa8\x88\xd2\x03rz2\xe2\xeahMn,\x96e{\xcaY\xfa\xf2\xe2\xdd\xee0,\xfb\xccF\xac\xc4c\xebo\x7f6+4s\x12\x94\x7f\xdf\xcc&\x9e\xf1r]PeY\x8fi\xb3\x87^2o\xc3\x18\x8a\xb5\x95\x8f\xb0z7\xdeZE;\xc76\xe1\xefg\xd19\xd3\x97[sb\x19H-\x87\x1c\x06\xfbt\x98 Is\xf0\x05\xd2\x1b\xb1\xa0\xa3\xb7\x1bp\xd3\x88i\xb9Z\xd68\xf4b\xdc%\x8bx\xbf\xf8\xe8~\xb8\\\xc9\x95O_\xa7-\xb9 k\xcfk\xdd\xf5\r\xf4\x82\xb7\xf4\xb2f.\x10(\xf6\x97Z{\x96\x16\x8f\x01\x19-ln\x7f9X\x06d\x83\x01\x83\x83\xc3M\x92\xdd2\xc6\x87\xa47\xe9\x16\xce\xaa-h\xaf\n\x81e:p\xc1\x9b\xadMmT\xca*-K\x85\x1b\xb3\x80\xe6pE\rk^5\xf0\x7f;\xb8\x9c\xe4\x04p\x89\x12%\x93\xda\n\x99\"`jc`Nv\x06\x98N\xbd\x83\xad\x1dX\x8a%\x85\xd2\xc7e\x1e-\x8e\xc6\xdf\xb6\xc6\xe1\x7f\x8a\x04!\x15)t\xf0>\x9c\x90\x9d\x0c.\xf1\x8a>Z\xaa\x0c\t\x1e\xc7\xd5<\xa3\xed\x97\xc3\x1e4\xfa8\xf9\x08\x0e\xe3\xc0]+\x83\xd1Vj\xc9\xb6\xa8TS.x2g=\x82\x7ft\xd0\xfb\xe1\xb6\x05`\xb9p\xdb\x8e\x0b\xf9\x13Xoq`\x10R\x10\xb9\xc1A\t\xefr\x1fg1x\xff\x96\x05\x8d\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=IdenTrust Commercial Root CA 1 O=IdenTrust
     * Subject: CN=IdenTrust Commercial Root CA 1 O=IdenTrust
     * Label: "IdenTrust Commercial Root CA 1"
     * Serial: 13298821034946342390520003877796839426
     * MD5 Fingerprint: b3:3e:77:73:75:ee:a0:d3:e3:7e:49:63:49:59:bb:c7
     * SHA1 Fingerprint: df:71:7e:aa:4a:d9:4e:c9:55:84:99:60:2d:48:de:5f:bc:f0:3a:25
     * SHA256 Fingerprint: 5d:56:49:9b:e4:d2:e0:8b:cf:ca:d0:8a:3e:38:72:3d:50:50:3b:de:70:69:48:e4:2f:55:60:30:19:e5:28:ae
     * -----BEGIN CERTIFICATE-----
     * MIIFYDCCA0igAwIBAgIQCgFCgAAAAUUjyES1AAAAAjANBgkqhkiG9w0BAQsFADBK
     * MQswCQYDVQQGEwJVUzESMBAGA1UEChMJSWRlblRydXN0MScwJQYDVQQDEx5JZGVu
     * VHJ1c3QgQ29tbWVyY2lhbCBSb290IENBIDEwHhcNMTQwMTE2MTgxMjIzWhcNMzQw
     * MTE2MTgxMjIzWjBKMQswCQYDVQQGEwJVUzESMBAGA1UEChMJSWRlblRydXN0MScw
     * JQYDVQQDEx5JZGVuVHJ1c3QgQ29tbWVyY2lhbCBSb290IENBIDEwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQCnUBneP5k91DNG8W9RYYKyqU+PZ4ldhNlT
     * 3Qwo2dfw/66VQ3KZ+bVdfIrBQuExUHTRgQ18zZshq0PirK1ehm7zCYofWjK9ouuU
     * +ehcCuz/mNKvcbO0U59Oh++SvL3sTzIwiEsXXlfEU8L2ApeN2WIrvyQfYo3fw7gp
     * S0l4PJNgiCL8mdo2yMKi1CxUAGc1bnO/AljwpN3lsKImesrgNqUZFvX9t++uP0D1
     * bVoE/c40yiTcdCMbXTMTEl3EASX2MN0CXZ/g1Ue9tOsbobtJSdifWwLziuQkkORi
     * T0/Br4sOdBeo0XKIanoBScy0RnnGF7HamB4HWfp1IYVl3ZBWzvurpWCdxJ35UrCL
     * vYf5jysjCiN2O/cz4ckA82n5S6LgTrx+kzmEB/dEcH7+B1rlsazRGMzyNeVJSQjK
     * Vsk9+w8YfYs7wRPCTY/JTw436R+hDmrfYi7LNQZReSzIJTj0+kuniVyc0uMNOYZK
     * dHzVWYfCP04MXFL0PfdSgvHqo6z9STQaKPNBiDoT7uje/5kdX7rL6B7yuVBgwDHT
     * c+XvvqDtMwt0viAgxGds8AgDelWAf0ZOlqf0Hj7h9tgJ4TNkK2PXMl6f+cB7D3hv
     * l7yTmvmcEpB4eoCHFddydJxVdHixuuFucAS6T6C6aMN7/zHwcz09lCqxC0EOoP5N
     * iGVreTO01wIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB
     * /zAdBgNVHQ4EFgQU7UQZwNPwBovupHu+QucmVMiONnYwDQYJKoZIhvcNAQELBQAD
     * ggIBAA2ukDL2pkt8RHYZYR4nKM1eVO8lvOMIkPkp165oCOGUAFjvLi5+U1KMtlwH
     * 6oi6mYtQlNeCgN9hCQCTrQ0U5s7B8jeUeLBfnLOic7iPBZM4zY0+sLj7wM+x8uwt
     * LRvM7Kqas6pgghstO8OEPVeKlh6cdbjTMM1gCIOQ045U8U1mwF10A0Cj7oV+wh93
     * nAbowacYXVKV7cndJZ5t+qntozo00Fl72u1Q8zW/7esUTTHHYPTa8Yec4kjixsU3
     * +wYQ+nVZZjFHKdp2mhzpgq7vmrlR94gjmmmVYjzlVYA211QC//G5Xc7UI2/YRYRK
     * W2XviQzdFKcgyxilJbQN+QHwotL0AMh0jqEqSI5l2xPE4iUXfeu+h1sXIFRRk0pT
     * AwvsXcoz7WL9RccvW9xYoIA55vrX/hMUpu09lEpCdNTDd1lzzY9GvlU47/rokTLq
     * l1gEIt44w8y8bckzOmoKaT+gyOpyj4xjhiO9bTyWnpXgSUyqorkqG5w2gXjtw+hG
     * 4iZZRHUe2XWJUc0QhJ1hYMtd+ZciTY6Y5uN/9lu7rs3KSoFrXgvzUeF0K+l+J6fZ
     * mUlO+KWA2yUPHGNiiskzZ2s8EIPGrd6ozRaOjfAHN3Gf8qv8QfXBi+wAN10J5U6A
     * 7/qxXDgGpRtK4dw4LTzcqx+QGtVKnO7RcGzM7vRX+Bi6hG6H
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x120\x10\x06\x03U\x04\n\x13\tIdenTrust1\'0%\x06\x03U\x04\x03\x13\x1eIdenTrust Commercial Root CA 1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa7P\x19\xde?\x99=\xd43F\xf1oQa\x82\xb2\xa9O\x8fg\x89]\x84\xd9S\xdd\x0c(\xd9\xd7\xf0\xff\xae\x95Cr\x99\xf9\xb5]|\x8a\xc1B\xe11Pt\xd1\x81\r|\xcd\x9b!\xabC\xe2\xac\xad^\x86n\xf3\t\x8a\x1fZ2\xbd\xa2\xeb\x94\xf9\xe8\\\n\xec\xff\x98\xd2\xafq\xb3\xb4S\x9fN\x87\xef\x92\xbc\xbd\xecO20\x88K\x17^W\xc4S\xc2\xf6\x02\x97\x8d\xd9b+\xbf$\x1fb\x8d\xdf\xc3\xb8)KIx<\x93`\x88\"\xfc\x99\xda6\xc8\xc2\xa2\xd4,T\x00g5ns\xbf\x02X\xf0\xa4\xdd\xe5\xb0\xa2&z\xca\xe06\xa5\x19\x16\xf5\xfd\xb7\xef\xae?@\xf5mZ\x04\xfd\xce4\xca$\xdct#\x1b]3\x13\x12]\xc4\x01%\xf60\xdd\x02]\x9f\xe0\xd5G\xbd\xb4\xeb\x1b\xa1\xbbII\xd8\x9f[\x02\xf3\x8a\xe4$\x90\xe4bOO\xc1\xaf\x8b\x0et\x17\xa8\xd1r\x88jz\x01I\xcc\xb4Fy\xc6\x17\xb1\xda\x98\x1e\x07Y\xfau!\x85e\xdd\x90V\xce\xfb\xab\xa5`\x9d\xc4\x9d\xf9R\xb0\x8b\xbd\x87\xf9\x8f+#\n#v;\xf73\xe1\xc9\x00\xf3i\xf9K\xa2\xe0N\xbc~\x939\x84\x07\xf7Dp~\xfe\x07Z\xe5\xb1\xac\xd1\x18\xcc\xf25\xe5II\x08\xcaV\xc9=\xfb\x0f\x18}\x8b;\xc1\x13\xc2M\x8f\xc9O\x0e7\xe9\x1f\xa1\x0ej\xdfb.\xcb5\x06Qy,\xc8%8\xf4\xfaK\xa7\x89\\\x9c\xd2\xe3\r9\x86Jt|\xd5Y\x87\xc2?N\x0c\\R\xf4=\xf7R\x82\xf1\xea\xa3\xac\xfdI4\x1a(\xf3A\x88:\x13\xee\xe8\xde\xff\x99\x1d_\xba\xcb\xe8\x1e\xf2\xb9P`\xc01\xd3s\xe5\xef\xbe\xa0\xed3\x0bt\xbe  \xc4gl\xf0\x08\x03zU\x80\x7fFN\x96\xa7\xf4\x1e>\xe1\xf6\xd8\t\xe13d+c\xd72^\x9f\xf9\xc0{\x0fxo\x97\xbc\x93\x9a\xf9\x9c\x12\x90xz\x80\x87\x15\xd7rt\x9cUtx\xb1\xba\xe1np\x04\xbaO\xa0\xbah\xc3{\xff1\xf0s==\x94*\xb1\x0bA\x0e\xa0\xfeM\x88eky3\xb4\xd7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=SwissSign Gold CA - G2 O=SwissSign AG
     * Subject: CN=SwissSign Gold CA - G2 O=SwissSign AG
     * Label: "SwissSign Gold CA - G2"
     * Serial: 13492815561806991280
     * MD5 Fingerprint: 24:77:d9:a8:91:d1:3b:fa:88:2d:c2:ff:f8:cd:33:93
     * SHA1 Fingerprint: d8:c5:38:8a:b7:30:1b:1b:6e:d4:7a:e6:45:25:3a:6f:9f:1a:27:61
     * SHA256 Fingerprint: 62:dd:0b:e9:b9:f5:0a:16:3e:a0:f8:e7:5c:05:3b:1e:ca:57:ea:55:c8:68:8f:64:7c:68:81:f2:c8:35:7b:95
     * -----BEGIN CERTIFICATE-----
     * MIIFujCCA6KgAwIBAgIJALtAHEP1Xk+wMA0GCSqGSIb3DQEBBQUAMEUxCzAJBgNV
     * BAYTAkNIMRUwEwYDVQQKEwxTd2lzc1NpZ24gQUcxHzAdBgNVBAMTFlN3aXNzU2ln
     * biBHb2xkIENBIC0gRzIwHhcNMDYxMDI1MDgzMDM1WhcNMzYxMDI1MDgzMDM1WjBF
     * MQswCQYDVQQGEwJDSDEVMBMGA1UEChMMU3dpc3NTaWduIEFHMR8wHQYDVQQDExZT
     * d2lzc1NpZ24gR29sZCBDQSAtIEcyMIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIIC
     * CgKCAgEAr+TufoskDhJuqVAtFkQ7kpJcyrhdhJJCEyq8ZVeCQD5XJM1QiyUqt2/8
     * 76LQwB8CJEoTlo8jE+YoWACjR8cGp4QjK7u9lit/VcyLwVcfDmJlD909Vopz2q5+
     * bbqBHH5CjCA12UNNhPqE21Is8w4ndwtrvxEvcnifLtg+5hg3Wipy+dpikJKVyh+c
     * 6bM8K8vzARO/Ws/BtQpgvd21mWRTuKCWs2/iJneRjOBiEAKfNA+k1ZIzUd6+jbqE
     * emA8atufK+ze3gE/bk3lUIbLtK/tREDFylqM2tIrfKjuvqblCqoOpd8FUrdVxyJd
     * MmqXl2MT28nbeTZ7hTpKxVKJ+STnnXepgv9VHKVxaSvRAiTysybUa9oEVeXBCsdt
     * MDeQKuSeFDNeFhdVxVu1yzSJkvGdJo+hB9TGsnhQ2wwMC3wLjEHXuendjIj3o02y
     * MszYF9rNt85mndT9Xv+9lz4pded+p2JYryU0pUHHPbwNUMoDAw8IWh+Vc3hiv69y
     * FGkOpeUDDniOJihC8AcLYiAQZzlG+qkDzAQ4embvIIO1jEpWjpEA/I5cgt6IoMPi
     * aG59je883WX0XaxR7ySArqpWl2/5rX3aYT+YdzylkbYcjCbaZaIJbcHiVOO5ykxM
     * gI93e2CaHt+28kgeDrpOVG2Y4OGiGqJ3UM/EY5LsRxmd6+ZrzsECAwEAAaOBrDCB
     * qTAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUWyV7
     * lqRlUX64OfPAeGZe6Drn8O4wHwYDVR0jBBgwFoAUWyV7lqRlUX64OfPAeGZe6Drn
     * 8O4wRgYDVR0gBD8wPTA7BglghXQBWQECAQEwLjAsBggrBgEFBQcCARYgaHR0cDov
     * L3JlcG9zaXRvcnkuc3dpc3NzaWduLmNvbS8wDQYJKoZIhvcNAQEFBQADggIBACe6
     * 45R88a7A3hfm5djV9VSwg/S7zV4Fe0+fdWavPOhWfvxyeDgD2StiGwC5+OlgzczO
     * UYrHUDFu4Up+GC9pWbY9ZIEr44OE5iKHjn3g7gKZYbge9LgriBIWhMIxkziWMaa5
     * O1M/wySTVltpkuzFwbs4AOPsF6m43Md8AYOfMke6UiI0HTJ6CVanfCU2qT1L2sCC
     * bwq7EsiHSycR+R4tx5M/nttfJmtS2S6K8RTGRI0Vqbe/vd6mGu6uLftIdxf+u+yv
     * GPUqUfA5hJeVbG4bwyvEdGB5JbAKJ9/fXtI5z0V9QkvfsywexcZdylU6oJxpmo/a
     * 77KwPJ+HbBIrZXAVUjEaJM9vMSNQH4xPjyPDdEFjHFWoFN0+4FFQz/EbMFYOkrCC
     * hdiDyyJkvC24JdVUorgG6q2SpCSgwYa1ShNqR88uC1aVVMvOmttqtKay20EIhid3
     * 92qgQmwLOM7XdVAyksLfKzAiSNDVQTglXaTpXZ/GlHXQRf0wl0OPkKsKx4ZzYEpp
     * Ld6leNcG2mqeSz53OiATIgHQv2ieY2BrNU0LbbqhPcCT4H8js1WtciVORvnSFu+w
     * ZMEBnunKoGqYDs/YYPIvSbjkQuE4NRb0yG5P94FW6LqjviOvrv1vA+ACOzB2+htt
     * Qc8Bsem4yWb02ybzOqR08kkkW8mw0FfB+j564ZfJ
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CH1\x150\x13\x06\x03U\x04\n\x13\x0cSwissSign AG1\x1f0\x1d\x06\x03U\x04\x03\x13\x16SwissSign Gold CA - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xaf\xe4\xee~\x8b$\x0e\x12n\xa9P-\x16D;\x92\x92\\\xca\xb8]\x84\x92B\x13*\xbceW\x82@>W$\xcdP\x8b%*\xb7o\xfc\xef\xa2\xd0\xc0\x1f\x02$J\x13\x96\x8f#\x13\xe6(X\x00\xa3G\xc7\x06\xa7\x84#+\xbb\xbd\x96+\x7fU\xcc\x8b\xc1W\x1f\x0ebe\x0f\xdd=V\x8as\xda\xae~m\xba\x81\x1c~B\x8c 5\xd9CM\x84\xfa\x84\xdbR,\xf3\x0e\'w\x0bk\xbf\x11/rx\x9f.\xd8>\xe6\x187Z*r\xf9\xdab\x90\x92\x95\xca\x1f\x9c\xe9\xb3<+\xcb\xf3\x01\x13\xbfZ\xcf\xc1\xb5\n`\xbd\xdd\xb5\x99dS\xb8\xa0\x96\xb3o\xe2&w\x91\x8c\xe0b\x10\x02\x9f4\x0f\xa4\xd5\x923Q\xde\xbe\x8d\xba\x84z`<j\xdb\x9f+\xec\xde\xde\x01?nM\xe5P\x86\xcb\xb4\xaf\xedD@\xc5\xcaZ\x8c\xda\xd2+|\xa8\xee\xbe\xa6\xe5\n\xaa\x0e\xa5\xdf\x05R\xb7U\xc7\"]2j\x97\x97c\x13\xdb\xc9\xdby6{\x85:J\xc5R\x89\xf9$\xe7\x9dw\xa9\x82\xffU\x1c\xa5qi+\xd1\x02$\xf2\xb3&\xd4k\xda\x04U\xe5\xc1\n\xc7m07\x90*\xe4\x9e\x143^\x16\x17U\xc5[\xb5\xcb4\x89\x92\xf1\x9d&\x8f\xa1\x07\xd4\xc6\xb2xP\xdb\x0c\x0c\x0b|\x0b\x8cA\xd7\xb9\xe9\xdd\x8c\x88\xf7\xa3M\xb22\xcc\xd8\x17\xda\xcd\xb7\xcef\x9d\xd4\xfd^\xff\xbd\x97>)u\xe7~\xa7bX\xaf%4\xa5A\xc7=\xbc\rP\xca\x03\x03\x0f\x08Z\x1f\x95sxb\xbf\xafr\x14i\x0e\xa5\xe5\x03\x0ex\x8e&(B\xf0\x07\x0bb \x10g9F\xfa\xa9\x03\xcc\x048zf\xef \x83\xb5\x8cJV\x8e\x91\x00\xfc\x8e\\\x82\xde\x88\xa0\xc3\xe2hn}\x8d\xef<\xdde\xf4]\xacQ\xef$\x80\xae\xaaV\x97o\xf9\xad}\xdaa?\x98w<\xa5\x91\xb6\x1c\x8c&\xdae\xa2\tm\xc1\xe2T\xe3\xb9\xcaLL\x80\x8fw{`\x9a\x1e\xdf\xb6\xf2H\x1e\x0e\xbaNTm\x98\xe0\xe1\xa2\x1a\xa2wP\xcf\xc4c\x92\xecG\x19\x9d\xeb\xe6k\xce\xc1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=CERTSIGN SA OU=certSIGN ROOT CA G2
     * Subject: O=CERTSIGN SA OU=certSIGN ROOT CA G2
     * Label: "certSIGN Root CA G2"
     * Serial: 313609486401300475190
     * MD5 Fingerprint: 8c:f1:75:8a:c6:19:cf:94:b7:f7:65:20:87:c3:97:c7
     * SHA1 Fingerprint: 26:f9:93:b4:ed:3d:28:27:b0:b9:4b:a7:e9:15:1d:a3:8d:92:e5:32
     * SHA256 Fingerprint: 65:7c:fe:2f:a7:3f:aa:38:46:25:71:f3:32:a2:36:3a:46:fc:e7:02:09:51:71:07:02:cd:fb:b6:ee:da:33:05
     * -----BEGIN CERTIFICATE-----
     * MIIFRzCCAy+gAwIBAgIJEQA0tk7GNi02MA0GCSqGSIb3DQEBCwUAMEExCzAJBgNV
     * BAYTAlJPMRQwEgYDVQQKEwtDRVJUU0lHTiBTQTEcMBoGA1UECxMTY2VydFNJR04g
     * Uk9PVCBDQSBHMjAeFw0xNzAyMDYwOTI3MzVaFw00MjAyMDYwOTI3MzVaMEExCzAJ
     * BgNVBAYTAlJPMRQwEgYDVQQKEwtDRVJUU0lHTiBTQTEcMBoGA1UECxMTY2VydFNJ
     * R04gUk9PVCBDQSBHMjCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAMDF
     * dRmRfUR0dIf+DjuW3NgBFszuY5HnC2/OOwppGnzC46+CjobXXo9X69MhWf05N0Iw
     * vlDqtg+piNguLWkh59E3GE59kdUWX2tbAMI5Qw02hVK5U2UPHULlj88F0+7cDBrZ
     * uIt4ImfkabBoxTzkbFpG583H+u/E7Eu9aqSs/cwoUe+StCmrqzWaTOTECMYmzPhp
     * n+Sc8CnTXPnGFiWeI8MgwT0PPzhAsP6CRDiqWhqKa2NYOLQV07YRaXseVO6MGiKs
     * cpc/I1mbySKEwQdPzH/iV8oScLumZfNpdWO9lfsbl83kqK/20U6o2YpxJM02PbyW
     * xPFsqa7lzw1uKA2wDrXKUXt4FMMgL3/7FFXhEZn91QqhngLjYl/rNUssuHLoPj1P
     * rCy7Lobio3aP5ZMqz6WryFyNSwb/EkaseMsUBzXgqd+L6a8VTxaJW732jcZZroiF
     * DsGJ6x9nxUWO/203Nit4ZoORUSs9/1F3dmKh7Gc+PoGD4FapUB8fepmrY7+EF3fx
     * DTvf95xhszWYijqy7DwaNz9+j5LP2RIUZNoQAhVB/0/E6xyjyfqZ90bp4RjZsbgy
     * LcsUDFDYg2WD7rlcz8sFWkz6GZdr1l0T08JcVLwyc6B49fFtHsufpaafItzRUZ6C
     * eWRgKRM+o/1Pcmqr4tTluCRVLERLiohEnMqE0yo7AgMBAAGjQjBAMA8GA1UdEwEB
     * /wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMB0GA1UdDgQWBBSCIS1mxteg4BXrzkwJ
     * d8RgnlRuAzANBgkqhkiG9w0BAQsFAAOCAgEAYN4auOfyYILVAzOBywaK8SJJ6ejq
     * kX/GM15oGQOGO0MBzwdw5AgeZYWR5hEit/UCI46uuR59H35s5r0l1ZUa8gWmr4UC
     * b6741jH/JclKyMeKqdmfS0mbEVeZkkMR3rYzpMzXjWR91M08KCy0mpbqTfXERMQl
     * qiCA2ClV9+BB/AYm/7k29UMUA2Z44RGx2iBfRgB4ACGlHgAoYXhvqAEBj500mv/0
     * OJD7uNGzcgbJceaBxXntC6Z58hMLnPddDnskk7RI24Zf3lCGeOdA5jGokHZwYa+c
     * NywRtYK3qq4kNFtyDGkNzVmf9nGvnAvRCjj5BiKDUyUM/FHE5r7iOZULJK2v0ZXk
     * ltd0ZGtxTgI8qoXzIKNDOXZbbFD+mpwUHmUUihW9o4JFWklWatKcsWMy5WHgUyIO
     * pwpJ6st+H6jiYoD2EEVSmAYY3qXNL3+q1Ok+CHLsIwMCPKaq2LxndD0UF/tUSxfj
     * 03k9bWtJySgOLnRQvwzZRjoQhsmnP+mg7H/rpXdYaXHmgwo38oZJar55CJD2AhZk
     * PuXaTH4MNMn5X7azKFGnpyuqSfqNZSlO42sTp5SjLVFteAxEy9/eCG/Oo2Sr05WE
     * 1LlSVHJ7liXMvGnjSG4N0MedJ5qq+BOS3R7fY581qRY27Iy4g/Q9iY/NtBde17MX
     * QRBdJ3NghVdJIgc=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02RO1\x140\x12\x06\x03U\x04\n\x13\x0bCERTSIGN SA1\x1c0\x1a\x06\x03U\x04\x0b\x13\x13certSIGN ROOT CA G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc0\xc5u\x19\x91}Dtt\x87\xfe\x0e;\x96\xdc\xd8\x01\x16\xcc\xeec\x91\xe7\x0bo\xce;\ni\x1a|\xc2\xe3\xaf\x82\x8e\x86\xd7^\x8fW\xeb\xd3!Y\xfd97B0\xbeP\xea\xb6\x0f\xa9\x88\xd8.-i!\xe7\xd17\x18N}\x91\xd5\x16_k[\x00\xc29C\r6\x85R\xb9Se\x0f\x1dB\xe5\x8f\xcf\x05\xd3\xee\xdc\x0c\x1a\xd9\xb8\x8bx\"g\xe4i\xb0h\xc5<\xe4lZF\xe7\xcd\xc7\xfa\xef\xc4\xecK\xbdj\xa4\xac\xfd\xcc(Q\xef\x92\xb4)\xab\xab5\x9aL\xe4\xc4\x08\xc6&\xcc\xf8i\x9f\xe4\x9c\xf0)\xd3\\\xf9\xc6\x16%\x9e#\xc3 \xc1=\x0f?8@\xb0\xfe\x82D8\xaaZ\x1a\x8akcX8\xb4\x15\xd3\xb6\x11i{\x1eT\xee\x8c\x1a\"\xacr\x97?#Y\x9b\xc9\"\x84\xc1\x07O\xcc\x7f\xe2W\xca\x12p\xbb\xa6e\xf3iuc\xbd\x95\xfb\x1b\x97\xcd\xe4\xa8\xaf\xf6\xd1N\xa8\xd9\x8aq$\xcd6=\xbc\x96\xc4\xf1l\xa9\xae\xe5\xcf\rn(\r\xb0\x0e\xb5\xcaQ{x\x14\xc3 /\x7f\xfb\x14U\xe1\x11\x99\xfd\xd5\n\xa1\x9e\x02\xe3b_\xeb5K,\xb8r\xe8>=O\xac,\xbb.\x86\xe2\xa3v\x8f\xe5\x93*\xcf\xa5\xab\xc8\\\x8dK\x06\xff\x12F\xacx\xcb\x14\x075\xe0\xa9\xdf\x8b\xe9\xaf\x15O\x16\x89[\xbd\xf6\x8d\xc6Y\xae\x88\x85\x0e\xc1\x89\xeb\x1fg\xc5E\x8e\xffm76+xf\x83\x91Q+=\xffQwvb\xa1\xecg>>\x81\x83\xe0V\xa9P\x1f\x1fz\x99\xabc\xbf\x84\x17w\xf1\r;\xdf\xf7\x9ca\xb35\x98\x8a:\xb2\xec<\x1a7?~\x8f\x92\xcf\xd9\x12\x14d\xda\x10\x02\x15A\xffO\xc4\xeb\x1c\xa3\xc9\xfa\x99\xf7F\xe9\xe1\x18\xd9\xb1\xb82-\xcb\x14\x0cP\xd8\x83e\x83\xee\xb9\\\xcf\xcb\x05ZL\xfa\x19\x97k\xd6]\x13\xd3\xc2\\T\xbc2s\xa0x\xf5\xf1m\x1e\xcb\x9f\xa5\xa6\x9f\"\xdc\xd1Q\x9e\x82yd`)\x13>\xa3\xfdOrj\xab\xe2\xd4\xe5\xb8$U,DK\x8a\x88D\x9c\xca\x84\xd3*;\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=OISTE WISeKey Global Root GB CA O=WISeKey OU=OISTE Foundation Endorsed
     * Subject: CN=OISTE WISeKey Global Root GB CA O=WISeKey OU=OISTE Foundation Endorsed
     * Label: "OISTE WISeKey Global Root GB CA"
     * Serial: 157768595616588414422159278966750757568
     * MD5 Fingerprint: a4:eb:b9:61:28:2e:b7:2f:98:b0:35:26:90:99:51:1d
     * SHA1 Fingerprint: 0f:f9:40:76:18:d3:d7:6a:4b:98:f0:a8:35:9e:0c:fd:27:ac:cc:ed
     * SHA256 Fingerprint: 6b:9c:08:e8:6e:b0:f7:67:cf:ad:65:cd:98:b6:21:49:e5:49:4a:67:f5:84:5e:7b:d1:ed:01:9f:27:b8:6b:d6
     * -----BEGIN CERTIFICATE-----
     * MIIDtTCCAp2gAwIBAgIQdrEgUnTwhYdGs/gjGvbCwDANBgkqhkiG9w0BAQsFADBt
     * MQswCQYDVQQGEwJDSDEQMA4GA1UEChMHV0lTZUtleTEiMCAGA1UECxMZT0lTVEUg
     * Rm91bmRhdGlvbiBFbmRvcnNlZDEoMCYGA1UEAxMfT0lTVEUgV0lTZUtleSBHbG9i
     * YWwgUm9vdCBHQiBDQTAeFw0xNDEyMDExNTAwMzJaFw0zOTEyMDExNTEwMzFaMG0x
     * CzAJBgNVBAYTAkNIMRAwDgYDVQQKEwdXSVNlS2V5MSIwIAYDVQQLExlPSVNURSBG
     * b3VuZGF0aW9uIEVuZG9yc2VkMSgwJgYDVQQDEx9PSVNURSBXSVNlS2V5IEdsb2Jh
     * bCBSb290IEdCIENBMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA2Be3
     * HEokKtaXscriHvt9OO+Y9bI5mE4nuBFde9IllIiCFSZqGzG7qFshISvYD06fWvGx
     * WuR51jIjK+FTzJlFXHtPrby/h0oLS5daqPZI7H17Dc0hBt+eFf1Biki3IPShehtX
     * 1F1Q/7pn2COZH8g/497/b1t3sWtuuMlk9+HKQUYOKXHQuSP8yYFfTvdv37+ErXNk
     * u7dCjmn21HYdfp2nuFeKUWdy19SouJVUQHMD9ur06/4oQnc/nSMbsrY9gBQHTC5P
     * 99UKFg29ZkM3fiNDecNAhvVMKdqOmq0NpQSHiB6F4+lT1ZvIiwNjeOvgGUpuuy9r
     * M2RYk61pv48b74JIxwIDAQABo1EwTzALBgNVHQ8EBAMCAYYwDwYDVR0TAQH/BAUw
     * AwEB/zAdBgNVHQ4EFgQUNQ/INmNe4qPs+TtmFc5RUuORmj0wEAYJKwYBBAGCNxUB
     * BAMCAQAwDQYJKoZIhvcNAQELBQADggEBAEBM+4eymYGQfp3FsLAmzYh7KzKNbrgh
     * cViXfa43FK8+5/ea4n32cZiZBKpDdHij40lhPnOMTZTg+XHEthYOU3gf1qKHLwI5
     * gSk8rxWYITD+KJAAjNHhy/peyP34EEY7onhCkRd0VQreUGdNZtGn//3ZwLWoo4rO
     * ZvUPQ82nK1d7Y0Zqqi5S2PTt4W2tKZB4SLrhI6qjiey1q5bAtEuiHZeeevJuQHHf
     * aPFlTc58Bd9TZaml8LGXBHAVRgOY1NK/VLSgWH1Sb9pWJmLU2NuJMW8c8CLC02Ic
     * Nc1MaRVUGpCY3useX8p3x8uOPUNpnJpY0CQ73xtAln41rYHHTnG6iBM=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CH1\x100\x0e\x06\x03U\x04\n\x13\x07WISeKey1\"0 \x06\x03U\x04\x0b\x13\x19OISTE Foundation Endorsed1(0&\x06\x03U\x04\x03\x13\x1fOISTE WISeKey Global Root GB CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd8\x17\xb7\x1cJ$*\xd6\x97\xb1\xca\xe2\x1e\xfb}8\xef\x98\xf5\xb29\x98N\'\xb8\x11]{\xd2%\x94\x88\x82\x15&j\x1b1\xbb\xa8[!!+\xd8\x0fN\x9fZ\xf1\xb1Z\xe4y\xd62#+\xe1S\xcc\x99E\\{O\xad\xbc\xbf\x87J\x0bK\x97Z\xa8\xf6H\xec}{\r\xcd!\x06\xdf\x9e\x15\xfdA\x8aH\xb7 \xf4\xa1z\x1bW\xd4]P\xff\xbag\xd8#\x99\x1f\xc8?\xe3\xde\xffo[w\xb1kn\xb8\xc9d\xf7\xe1\xcaAF\x0e)q\xd0\xb9#\xfc\xc9\x81_N\xf7o\xdf\xbf\x84\xadsd\xbb\xb7B\x8ei\xf6\xd4v\x1d~\x9d\xa7\xb8W\x8aQgr\xd7\xd4\xa8\xb8\x95T@s\x03\xf6\xea\xf4\xeb\xfe(Bw?\x9d#\x1b\xb2\xb6=\x80\x14\x07L.O\xf7\xd5\n\x16\r\xbdfC7~#Cy\xc3@\x86\xf5L)\xda\x8e\x9a\xad\r\xa5\x04\x87\x88\x1e\x85\xe3\xe9S\xd5\x9b\xc8\x8b\x03cx\xeb\xe0\x19Jn\xbb/k3dX\x93\xadi\xbf\x8f\x1b\xef\x82H\xc7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=NetLock Arany (Class Gold) Főtanúsítvány O=NetLock Kft. OU=Tanúsítványkiadók (Certification Services)
     * Subject: CN=NetLock Arany (Class Gold) Főtanúsítvány O=NetLock Kft. OU=Tanúsítványkiadók (Certification Services)
     * Label: "NetLock Arany (Class Gold) Főtanúsítvány"
     * Serial: 80544274841616
     * MD5 Fingerprint: c5:a1:b7:ff:73:dd:d6:d7:34:32:18:df:fc:3c:ad:88
     * SHA1 Fingerprint: 06:08:3f:59:3f:15:a1:04:a0:69:a4:6b:a9:03:d0:06:b7:97:09:91
     * SHA256 Fingerprint: 6c:61:da:c3:a2:de:f0:31:50:6b:e0:36:d2:a6:fe:40:19:94:fb:d1:3d:f9:c8:d4:66:59:92:74:c4:46:ec:98
     * -----BEGIN CERTIFICATE-----
     * MIIEFTCCAv2gAwIBAgIGSUEs5AAQMA0GCSqGSIb3DQEBCwUAMIGnMQswCQYDVQQG
     * EwJIVTERMA8GA1UEBwwIQnVkYXBlc3QxFTATBgNVBAoMDE5ldExvY2sgS2Z0LjE3
     * MDUGA1UECwwuVGFuw7pzw610dsOhbnlraWFkw7NrIChDZXJ0aWZpY2F0aW9uIFNl
     * cnZpY2VzKTE1MDMGA1UEAwwsTmV0TG9jayBBcmFueSAoQ2xhc3MgR29sZCkgRsWR
     * dGFuw7pzw610dsOhbnkwHhcNMDgxMjExMTUwODIxWhcNMjgxMjA2MTUwODIxWjCB
     * pzELMAkGA1UEBhMCSFUxETAPBgNVBAcMCEJ1ZGFwZXN0MRUwEwYDVQQKDAxOZXRM
     * b2NrIEtmdC4xNzA1BgNVBAsMLlRhbsO6c8OtdHbDoW55a2lhZMOzayAoQ2VydGlm
     * aWNhdGlvbiBTZXJ2aWNlcykxNTAzBgNVBAMMLE5ldExvY2sgQXJhbnkgKENsYXNz
     * IEdvbGQpIEbFkXRhbsO6c8OtdHbDoW55MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8A
     * MIIBCgKCAQEAxCRec75LbRTDofTjl5Bu0jBFHjzuZ9lk4BqKf8owyoPjIMHj9DrT
     * lF8afFttvzBPhCf2nx9JvMaZCpDyD/V/Q4Q3Y1GLeqVw/HpYzY6b7cNGbIRwXdrz
     * AZAj/E4wqX7hJ2Pn7WQ8oLjJM2P+FpD/sLj916jAwJRDC7bVWaaeVtAkH3B5r9s5
     * VA1lddkVQZQBr17s9o3x/61k/iCa11zr/qYfCGSji3ZVrR47KGAuhyXoqq8fxmRG
     * ILdwfzzeSNuWU7c5d+Qa4scWhHaXWy+7GRWF+GmF9ZmnqfI0p6m2pgP8b4Y9VHx2
     * BJtr+UBdADTHLpl1neWIA6pN+APSQnbAGwIDAKiLo0UwQzASBgNVHRMBAf8ECDAG
     * AQH/AgEEMA4GA1UdDwEB/wQEAwIBBjAdBgNVHQ4EFgQUzPpnk/C2uNClwB7zU/2M
     * U9+D15YwDQYJKoZIhvcNAQELBQADggEBAKt/7hwWqZw8UQCgwBEIBaeZ5m8BiFRh
     * bvG5GK1Krf6BQCOUL/t1fC8oS2IkgYIL9WHxHG64YTjrgfpioTtaYtOUZcTh5m2C
     * +C8lcLIhJsFyUR+MLMOEkMNaj7rP9KdlpeuY0fsFskZ1FSNqb4VjMIDw1Z4fKRzC
     * bLBQWV2QWzuoDTDPv31/zvGdg73JRm4gpvlhUbohL3u+pRVjodSVh/GeufOJ8z2F
     * uLjbvrW5KfnaNwUASZQDhETnv0Mxz3WLJdH0pmT1kvarBes96aULNmLazAZfNou2
     * XjG4Kvte9nHfRCaexOYNkbQudZWAUWpLMKawYqGT8ZvYzsRjdT9ZR7E=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02HU1\x110\x0f\x06\x03U\x04\x07\x0c\x08Budapest1\x150\x13\x06\x03U\x04\n\x0c\x0cNetLock Kft.1705\x06\x03U\x04\x0b\x0c.Tan\xc3\xbas\xc3\xadtv\xc3\xa1nykiad\xc3\xb3k (Certification Services)1503\x06\x03U\x04\x03\x0c,NetLock Arany (Class Gold) F\xc5\x91tan\xc3\xbas\xc3\xadtv\xc3\xa1ny",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xc4$^s\xbeKm\x14\xc3\xa1\xf4\xe3\x97\x90n\xd20E\x1e<\xeeg\xd9d\xe0\x1a\x8a\x7f\xca0\xca\x83\xe3 \xc1\xe3\xf4:\xd3\x94_\x1a|[m\xbf0O\x84\'\xf6\x9f\x1fI\xbc\xc6\x99\n\x90\xf2\x0f\xf5\x7fC\x847cQ\x8bz\xa5p\xfczX\xcd\x8e\x9b\xed\xc3Fl\x84p]\xda\xf3\x01\x90#\xfcN0\xa9~\xe1\'c\xe7\xedd<\xa0\xb8\xc93c\xfe\x16\x90\xff\xb0\xb8\xfd\xd7\xa8\xc0\xc0\x94C\x0b\xb6\xd5Y\xa6\x9eV\xd0$\x1fpy\xaf\xdb9T\reu\xd9\x15A\x94\x01\xaf^\xec\xf6\x8d\xf1\xff\xadd\xfe \x9a\xd7\\\xeb\xfe\xa6\x1f\x08d\xa3\x8bvU\xad\x1e;(`.\x87%\xe8\xaa\xaf\x1f\xc6dF \xb7p\x7f<\xdeH\xdb\x96S\xb79w\xe4\x1a\xe2\xc7\x16\x84v\x97[/\xbb\x19\x15\x85\xf8i\x85\xf5\x99\xa7\xa9\xf24\xa7\xa9\xb6\xa6\x03\xfco\x86=T|v\x04\x9bk\xf9@]\x004\xc7.\x99u\x9d\xe5\x88\x03\xaaM\xf8\x03\xd2Bv\xc0\x1b\x02\x03\x00\xa8\x8b",
        name_constraints: None
    },

    /*
     * Issuer: CN=Entrust.net Certification Authority (2048) O=Entrust.net OU=www.entrust.net/CPS_2048 incorp. by ref. (limits liab.)/(c) 1999 Entrust.net Limited
     * Subject: CN=Entrust.net Certification Authority (2048) O=Entrust.net OU=www.entrust.net/CPS_2048 incorp. by ref. (limits liab.)/(c) 1999 Entrust.net Limited
     * Label: "Entrust.net Premium 2048 Secure Server CA"
     * Serial: 946069240
     * MD5 Fingerprint: ee:29:31:bc:32:7e:9a:e6:e8:b5:f7:51:b4:34:71:90
     * SHA1 Fingerprint: 50:30:06:09:1d:97:d4:f5:ae:39:f7:cb:e7:92:7d:7d:65:2d:34:31
     * SHA256 Fingerprint: 6d:c4:71:72:e0:1c:bc:b0:bf:62:58:0d:89:5f:e2:b8:ac:9a:d4:f8:73:80:1e:0c:10:b9:c8:37:d2:1e:b1:77
     * -----BEGIN CERTIFICATE-----
     * MIIEKjCCAxKgAwIBAgIEOGPe+DANBgkqhkiG9w0BAQUFADCBtDEUMBIGA1UEChML
     * RW50cnVzdC5uZXQxQDA+BgNVBAsUN3d3dy5lbnRydXN0Lm5ldC9DUFNfMjA0OCBp
     * bmNvcnAuIGJ5IHJlZi4gKGxpbWl0cyBsaWFiLikxJTAjBgNVBAsTHChjKSAxOTk5
     * IEVudHJ1c3QubmV0IExpbWl0ZWQxMzAxBgNVBAMTKkVudHJ1c3QubmV0IENlcnRp
     * ZmljYXRpb24gQXV0aG9yaXR5ICgyMDQ4KTAeFw05OTEyMjQxNzUwNTFaFw0yOTA3
     * MjQxNDE1MTJaMIG0MRQwEgYDVQQKEwtFbnRydXN0Lm5ldDFAMD4GA1UECxQ3d3d3
     * LmVudHJ1c3QubmV0L0NQU18yMDQ4IGluY29ycC4gYnkgcmVmLiAobGltaXRzIGxp
     * YWIuKTElMCMGA1UECxMcKGMpIDE5OTkgRW50cnVzdC5uZXQgTGltaXRlZDEzMDEG
     * A1UEAxMqRW50cnVzdC5uZXQgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkgKDIwNDgp
     * MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEArU1LqRKGsuqjIAcVFmQq
     * K0vRvwtKTY7tgHalZ7d4QMBzQshowNtTK91euHaYNZOLGp18EzoOH1u3Hs/lJBQe
     * sYGpjX24zGtLA/ECDNyrpUAkAH90lKGdCCmziAv1h3edVc3kw37XamSrhRSGlVuX
     * MlBvPci6Zgzj/L24ScF2iUkZ/cCovYmjZy/Gn7xxGWC4LeksyZB2ZnuU4q941mVT
     * XTzWnLLPKQP5L6RQstRIzgUyVYr9smRMDuSYB3Xbf9+5CFVghTAp+XtIpGmG4zU/
     * HoZdenoVve8AjhUiVBcAkCaTvA5JaJG/+EfTnZVCwQ5N328mz8MYIWJmQ3DW1cAH
     * 4QIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNV
     * HQ4EFgQUVeSB0RGAvtiJuQijMfmhJAkWuXAwDQYJKoZIhvcNAQEFBQADggEBADub
     * j1abMOdTmXx6eadNl9cZlZD7Bh/KM3xGY4+WZiT6QBshJ8rmcnPyT/4xmf3IDExo
     * U8aAghOY+rat2l098c5u9hURlIIM7j+VrxGrD9cv3h8Dj1csHsm7mhpElesYT6Yf
     * zX1XEC+bBAlahLVu2B064dae0Wx5XnkcFMXj0EyTO2U87d89vqbllRrDtRnDvV5b
     * u/8j72gZyxKTJ1wDLW8w0B62GqzeWvfRqqgnpv55gcR5mTNXuhKwqeBCbJPKVt7+
     * bYQLCIt+jerXmCHG8+c8eS9enNFMFY3h7CI3zJpDC5fcgJCNs2ebb0gIFVbPv/Er
     * fF6adulZkMV8gzURZVE=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x140\x12\x06\x03U\x04\n\x13\x0bEntrust.net1@0>\x06\x03U\x04\x0b\x147www.entrust.net/CPS_2048 incorp. by ref. (limits liab.)1%0#\x06\x03U\x04\x0b\x13\x1c(c) 1999 Entrust.net Limited1301\x06\x03U\x04\x03\x13*Entrust.net Certification Authority (2048)",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xadMK\xa9\x12\x86\xb2\xea\xa3 \x07\x15\x16d*+K\xd1\xbf\x0bJM\x8e\xed\x80v\xa5g\xb7x@\xc0sB\xc8h\xc0\xdbS+\xdd^\xb8v\x985\x93\x8b\x1a\x9d|\x13:\x0e\x1f[\xb7\x1e\xcf\xe5$\x14\x1e\xb1\x81\xa9\x8d}\xb8\xcckK\x03\xf1\x02\x0c\xdc\xab\xa5@$\x00\x7ft\x94\xa1\x9d\x08)\xb3\x88\x0b\xf5\x87w\x9dU\xcd\xe4\xc3~\xd7jd\xab\x85\x14\x86\x95[\x972Po=\xc8\xbaf\x0c\xe3\xfc\xbd\xb8I\xc1v\x89I\x19\xfd\xc0\xa8\xbd\x89\xa3g/\xc6\x9f\xbcq\x19`\xb8-\xe9,\xc9\x90vf{\x94\xe2\xafx\xd6eS]<\xd6\x9c\xb2\xcf)\x03\xf9/\xa4P\xb2\xd4H\xce\x052U\x8a\xfd\xb2dL\x0e\xe4\x98\x07u\xdb\x7f\xdf\xb9\x08U`\x850)\xf9{H\xa4i\x86\xe35?\x1e\x86]zz\x15\xbd\xef\x00\x8e\x15\"T\x17\x00\x90&\x93\xbc\x0eIh\x91\xbf\xf8G\xd3\x9d\x95B\xc1\x0eM\xdfo&\xcf\xc3\x18!bfCp\xd6\xd5\xc0\x07\xe1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=AffirmTrust Premium O=AffirmTrust
     * Subject: CN=AffirmTrust Premium O=AffirmTrust
     * Label: "AffirmTrust Premium"
     * Serial: 7893706540734352110
     * MD5 Fingerprint: c4:5d:0e:48:b6:ac:28:30:4e:0a:bc:f9:38:16:87:57
     * SHA1 Fingerprint: d8:a6:33:2c:e0:03:6f:b1:85:f6:63:4f:7d:6a:06:65:26:32:28:27
     * SHA256 Fingerprint: 70:a7:3f:7f:37:6b:60:07:42:48:90:45:34:b1:14:82:d5:bf:0e:69:8e:cc:49:8d:f5:25:77:eb:f2:e9:3b:9a
     * -----BEGIN CERTIFICATE-----
     * MIIFRjCCAy6gAwIBAgIIbYwURrGmCu4wDQYJKoZIhvcNAQEMBQAwQTELMAkGA1UE
     * BhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MRwwGgYDVQQDDBNBZmZpcm1UcnVz
     * dCBQcmVtaXVtMB4XDTEwMDEyOTE0MTAzNloXDTQwMTIzMTE0MTAzNlowQTELMAkG
     * A1UEBhMCVVMxFDASBgNVBAoMC0FmZmlybVRydXN0MRwwGgYDVQQDDBNBZmZpcm1U
     * cnVzdCBQcmVtaXVtMIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAxBLf
     * qV/+Qd3d9Z+K4/as4Tx4mrzY8H96oDMq3I0gW64tb+eT2TZwamjPjlGjhVtnBKAQ
     * JG9dKILBl1fYSCkTtuG+kU3fhQxTGJoeJKJPj/CihQvL9Cl/0qRY7iZNyaqoe5rZ
     * +jjeRFcV5fiMyNlI4g0WJx0eyIOFJbe6qlVBzAMiSy2RjYvmia9mx+n/K+k8rNrS
     * s8PhaJyJ+HoAVt70VZVs+7pk3WKL3wt3MutizCaam7uqYoNMtAZ6MMgpv+0GTZe5
     * HMQxK9VfvFMSF5yZVylmd2EhMQcuJUmdGPLu8ytxjLW6OQdJd/zvLpKQBY0tL3d7
     * 70O/Nbua2Plzpyzy0FfuKE4mX4+QaAkvuPjcBukumj5Rp9EixAqnOEhss/n/fauG
     * V+O61oV4d7pD6kh/9ti+I20ev9E2bFhc8e6kGVQa9QPSdubhjL08s9NIS+LI+H+S
     * qHZGnEJlPqQewQcDWkYtuJfzt9WyVSHvutxMAJf7FJUnM7/oQ0dG0giZFmA7mn7S
     * 5u046uwBHjxIVkkJx0w3AJ6IDsBz4W9m6XJHMD4Q5QsDyZpCAGzFlH5hxIrff4Ia
     * C1nEWTJ3s7xgaVY5/bQGeyzWZDbZvUjthB9+pSKPKrhC9IK31FOQeE4tGv2Bb0TX
     * OwF0lkLgAOIua+rF7nKsu7/+6qqo+Nz2snmKtmcCAwEAAaNCMEAwHQYDVR0OBBYE
     * FJ3AZ6YMItkm9UWrpmVSESfYRaxjMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/
     * BAQDAgEGMA0GCSqGSIb3DQEBDAUAA4ICAQCzV00QYk465KzquByvMiPIs0laUZx2
     * KI15qldGF9X1Uva3ROgIRL8YhNILgM3FEv0AVQVhh0HctSSePMTYyPtwni94loMg
     * Nt58D2kTiKV1NpgIpsbfrM7jWNa3Pt668+s0QNiigfV4Py/VpfzZotReBA4Xrf5B
     * 8OWycvpEgjNC6C1Y91aMYj+6QrCcDFx+LmUmXFNPALJ4fqENmS2NuB2OosSw/WDQ
     * MKSOyARiqcTtNd56l+0OOF6SL5Nwpamcb6d9Ex1+xghIsV5n61EIJenmJWtSKZGc
     * 0jlzCFfemQa0W50QBuHCAKi4HEoCChTQwUHK+4w1IX2COPKpVJEZNZOUbWo6xbLQ
     * u4mGk+ibyQ86p3q4ofB4Rvr8Ny/lioTz3/4E2aFooC8k4gmVBtWVyuEklut89pMF
     * u+1z6S3RdTnX5yTb2E5fQ4+e0BQ5v1VwSJlXMbSc7kqYA5YwH2AG7hsj/oFgIxpH
     * YoWlzBk0gG+zrBrjn/B7SK3VAdlntqlyk+otZrWyuOQ9PLLvTIzq6we/qzWaVYa8
     * GKa1qF60g2xraUDTn9zxw2lrueFtCfTxqlB2Cnp9ehehVZZCmTEJ3WARjQUwfuaO
     * RtGdFNrHF+QFlozEJLUbzxQHskD4o55BhrwE0GuWyCqANP2/7waj3VjFhT0+j/6e
     * KeC2uAloGRwYQw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x140\x12\x06\x03U\x04\n\x0c\x0bAffirmTrust1\x1c0\x1a\x06\x03U\x04\x03\x0c\x13AffirmTrust Premium",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc4\x12\xdf\xa9_\xfeA\xdd\xdd\xf5\x9f\x8a\xe3\xf6\xac\xe1<x\x9a\xbc\xd8\xf0\x7fz\xa03*\xdc\x8d [\xae-o\xe7\x93\xd96pjh\xcf\x8eQ\xa3\x85[g\x04\xa0\x10$o](\x82\xc1\x97W\xd8H)\x13\xb6\xe1\xbe\x91M\xdf\x85\x0cS\x18\x9a\x1e$\xa2O\x8f\xf0\xa2\x85\x0b\xcb\xf4)\x7f\xd2\xa4X\xee&M\xc9\xaa\xa8{\x9a\xd9\xfa8\xdeDW\x15\xe5\xf8\x8c\xc8\xd9H\xe2\r\x16\'\x1d\x1e\xc8\x83\x85%\xb7\xba\xaaUA\xcc\x03\"K-\x91\x8d\x8b\xe6\x89\xaff\xc7\xe9\xff+\xe9<\xac\xda\xd2\xb3\xc3\xe1h\x9c\x89\xf8z\x00V\xde\xf4U\x95l\xfb\xbad\xddb\x8b\xdf\x0bw2\xebb\xcc&\x9a\x9b\xbb\xaab\x83L\xb4\x06z0\xc8)\xbf\xed\x06M\x97\xb9\x1c\xc41+\xd5_\xbcS\x12\x17\x9c\x99W)fwa!1\x07.%I\x9d\x18\xf2\xee\xf3+q\x8c\xb5\xba9\x07Iw\xfc\xef.\x92\x90\x05\x8d-/w{\xefC\xbf5\xbb\x9a\xd8\xf9s\xa7,\xf2\xd0W\xee(N&_\x8f\x90h\t/\xb8\xf8\xdc\x06\xe9.\x9a>Q\xa7\xd1\"\xc4\n\xa78Hl\xb3\xf9\xff}\xab\x86W\xe3\xba\xd6\x85xw\xbaC\xeaH\x7f\xf6\xd8\xbe#m\x1e\xbf\xd16lX\\\xf1\xee\xa4\x19T\x1a\xf5\x03\xd2v\xe6\xe1\x8c\xbd<\xb3\xd3HK\xe2\xc8\xf8\x7f\x92\xa8vF\x9cBe>\xa4\x1e\xc1\x07\x03ZF-\xb8\x97\xf3\xb7\xd5\xb2U!\xef\xba\xdcL\x00\x97\xfb\x14\x95\'3\xbf\xe8CGF\xd2\x08\x99\x16`;\x9a~\xd2\xe6\xed8\xea\xec\x01\x1e<HVI\t\xc7L7\x00\x9e\x88\x0e\xc0s\xe1of\xe9rG0>\x10\xe5\x0b\x03\xc9\x9aB\x00l\xc5\x94~a\xc4\x8a\xdf\x7f\x82\x1a\x0bY\xc4Y2w\xb3\xbc`iV9\xfd\xb4\x06{,\xd6d6\xd9\xbdH\xed\x84\x1f~\xa5\"\x8f*\xb8B\xf4\x82\xb7\xd4S\x90xN-\x1a\xfd\x81oD\xd7;\x01t\x96B\xe0\x00\xe2.k\xea\xc5\xeer\xac\xbb\xbf\xfe\xea\xaa\xa8\xf8\xdc\xf6\xb2y\x8a\xb6g\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GTS Root R4 O=Google Trust Services LLC
     * Subject: CN=GTS Root R4 O=Google Trust Services LLC
     * Label: "GTS Root R4"
     * Serial: 146587176229350439916519468929765261721
     * MD5 Fingerprint: 5d:b6:6a:c4:60:17:24:6a:1a:99:a8:4b:ee:5e:b4:26
     * SHA1 Fingerprint: 2a:1d:60:27:d9:4a:b1:0a:1c:4d:91:5c:cd:33:a0:cb:3e:2d:54:cb
     * SHA256 Fingerprint: 71:cc:a5:39:1f:9e:79:4b:04:80:25:30:b3:63:e1:21:da:8a:30:43:bb:26:66:2f:ea:4d:ca:7f:c9:51:a4:bd
     * -----BEGIN CERTIFICATE-----
     * MIICCjCCAZGgAwIBAgIQbkepyIuUtui7OyrYorLBmTAKBggqhkjOPQQDAzBHMQsw
     * CQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZpY2VzIExMQzEU
     * MBIGA1UEAxMLR1RTIFJvb3QgUjQwHhcNMTYwNjIyMDAwMDAwWhcNMzYwNjIyMDAw
     * MDAwWjBHMQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZp
     * Y2VzIExMQzEUMBIGA1UEAxMLR1RTIFJvb3QgUjQwdjAQBgcqhkjOPQIBBgUrgQQA
     * IgNiAATzdHOnaItgrkO4NcWBMHtLSZ37wWHO5t5GvWvVYRg1rkDdc/eJkTBa6zzu
     * hXyiQHY7qca4R9gq55KRanPpsXI5nymfopjTX15YhmUPoYRlBtHci8nHc8iMai/l
     * xKvRHYqjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTADAQH/MB0GA1Ud
     * DgQWBBSATNbrdP9JNqPV2Py1PsVq8JQdjDAKBggqhkjOPQQDAwNnADBkAjBqUFJ0
     * CMRw3J5QdCHojXohw0+WbhXRIjVhLfoIN+4Zba3bssx9BzT1YBkstTTZbyACMANx
     * sbqjYAuG7ZoIapVon+Kz4ZNkfF6Tpt95LY2F45TPI11xzPKwTdb+mciUqXWi4w==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\"0 \x06\x03U\x04\n\x13\x19Google Trust Services LLC1\x140\x12\x06\x03U\x04\x03\x13\x0bGTS Root R4",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xf3ts\xa7h\x8b`\xaeC\xb85\xc5\x810{KI\x9d\xfb\xc1a\xce\xe6\xdeF\xbdk\xd5a\x185\xae@\xdds\xf7\x89\x910Z\xeb<\xee\x85|\xa2@v;\xa9\xc6\xb8G\xd8*\xe7\x92\x91js\xe9\xb1r9\x9f)\x9f\xa2\x98\xd3_^X\x86e\x0f\xa1\x84e\x06\xd1\xdc\x8b\xc9\xc7s\xc8\x8cj/\xe5\xc4\xab\xd1\x1d\x8a",
        name_constraints: None
    },

    /*
     * Issuer: CN=Entrust Root Certification Authority O=Entrust, Inc. OU=www.entrust.net/CPS is incorporated by reference/(c) 2006 Entrust, Inc.
     * Subject: CN=Entrust Root Certification Authority O=Entrust, Inc. OU=www.entrust.net/CPS is incorporated by reference/(c) 2006 Entrust, Inc.
     * Label: "Entrust Root Certification Authority"
     * Serial: 1164660820
     * MD5 Fingerprint: d6:a5:c3:ed:5d:dd:3e:00:c1:3d:87:92:1f:1d:3f:e4
     * SHA1 Fingerprint: b3:1e:b1:b7:40:e3:6c:84:02:da:dc:37:d4:4d:f5:d4:67:49:52:f9
     * SHA256 Fingerprint: 73:c1:76:43:4f:1b:c6:d5:ad:f4:5b:0e:76:e7:27:28:7c:8d:e5:76:16:c1:e6:e6:14:1a:2b:2c:bc:7d:8e:4c
     * -----BEGIN CERTIFICATE-----
     * MIIEkTCCA3mgAwIBAgIERWtQVDANBgkqhkiG9w0BAQUFADCBsDELMAkGA1UEBhMC
     * VVMxFjAUBgNVBAoTDUVudHJ1c3QsIEluYy4xOTA3BgNVBAsTMHd3dy5lbnRydXN0
     * Lm5ldC9DUFMgaXMgaW5jb3Jwb3JhdGVkIGJ5IHJlZmVyZW5jZTEfMB0GA1UECxMW
     * KGMpIDIwMDYgRW50cnVzdCwgSW5jLjEtMCsGA1UEAxMkRW50cnVzdCBSb290IENl
     * cnRpZmljYXRpb24gQXV0aG9yaXR5MB4XDTA2MTEyNzIwMjM0MloXDTI2MTEyNzIw
     * NTM0MlowgbAxCzAJBgNVBAYTAlVTMRYwFAYDVQQKEw1FbnRydXN0LCBJbmMuMTkw
     * NwYDVQQLEzB3d3cuZW50cnVzdC5uZXQvQ1BTIGlzIGluY29ycG9yYXRlZCBieSBy
     * ZWZlcmVuY2UxHzAdBgNVBAsTFihjKSAyMDA2IEVudHJ1c3QsIEluYy4xLTArBgNV
     * BAMTJEVudHJ1c3QgUm9vdCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTCCASIwDQYJ
     * KoZIhvcNAQEBBQADggEPADCCAQoCggEBALaVtkNC+sZtKm9I35RMOVcF7sN5EUFo
     * Nu3s/poBj6E4KPz3EEZmLk0eGrEaTsbRwJWIsMn/MYszA9u3g3s+IIRe7bJWKKf4
     * 4LlAcTfFy0cOlypowCKVYhXbR9n10Cv/gkvJrT7eTNuQgFA/CYqEAOwwCj0Yzfv9
     * KlmaI5UXLEWeH25DeW0MXJj+SKfFI0dcXv1u5x609mhF0YaDW6KKjbHjKYD+JXGI
     * rb68j6xSlkuqUY3kEzEZ6E5Nn9uss2rVvDlUccp6en+Q3X0dgNmBu1kmwhH+5pPi
     * 94DkZfs0Nw4pgHBNrziGLp5/V6+eF67rHMsoIV+2HNjnogQi+dPa2MsCAwEAAaOB
     * sDCBrTAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zArBgNVHRAEJDAi
     * gA8yMDA2MTEyNzIwMjM0MlqBDzIwMjYxMTI3MjA1MzQyWjAfBgNVHSMEGDAWgBRo
     * kORnpKZTgMeGZqTx90tD+4S9bTAdBgNVHQ4EFgQUaJDkZ6SmU4DHhmak8fdLQ/uE
     * vW0wHQYJKoZIhvZ9B0EABBAwDhsIVjcuMTo0LjADAgSQMA0GCSqGSIb3DQEBBQUA
     * A4IBAQCT1DCw1wMgKtD5Y+iRDAUgqV8ZyntyTtSx29CW+1RaGSwMCPeyvIWonX9t
     * O1KzKtvn1ISMY/YPyyYBkVBs9F8U4pN0wBOeMDpQ47RgxRzwIkSNcUesyBrJ6Zua
     * AGAT/3B+XxFNSRuzFVJ7yVTav52Vr2ua2J7p8eRDjeIRRDq/r72DQnNSi6q7pynP
     * 9WQcCk3RvKqsnyrQ/39/2n3qse0wJcGE2jTSW3iDVuycNsMm4hH2Z0kdkquM++v/
     * eu6FSqdQgPCnXEqULl8FmTxSQeDNtGPPAUO6nIPcj2A781q0tHuu2guQOHXvgR1m
     * 0vdXcDazv/wor3ElhVsT/h5/WrQ8
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x160\x14\x06\x03U\x04\n\x13\rEntrust, Inc.1907\x06\x03U\x04\x0b\x130www.entrust.net/CPS is incorporated by reference1\x1f0\x1d\x06\x03U\x04\x0b\x13\x16(c) 2006 Entrust, Inc.1-0+\x06\x03U\x04\x03\x13$Entrust Root Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb6\x95\xb6CB\xfa\xc6m*oH\xdf\x94L9W\x05\xee\xc3y\x11Ah6\xed\xec\xfe\x9a\x01\x8f\xa18(\xfc\xf7\x10Ff.M\x1e\x1a\xb1\x1aN\xc6\xd1\xc0\x95\x88\xb0\xc9\xff1\x8b3\x03\xdb\xb7\x83{> \x84^\xed\xb2V(\xa7\xf8\xe0\xb9@q7\xc5\xcbG\x0e\x97*h\xc0\"\x95b\x15\xdbG\xd9\xf5\xd0+\xff\x82K\xc9\xad>\xdeL\xdb\x90\x80P?\t\x8a\x84\x00\xec0\n=\x18\xcd\xfb\xfd*Y\x9a#\x95\x17,E\x9e\x1fnCym\x0c\\\x98\xfeH\xa7\xc5#G\\^\xfdn\xe7\x1e\xb4\xf6hE\xd1\x86\x83[\xa2\x8a\x8d\xb1\xe3)\x80\xfe%q\x88\xad\xbe\xbc\x8f\xacR\x96K\xaaQ\x8d\xe4\x131\x19\xe8NM\x9f\xdb\xac\xb3j\xd5\xbc9Tq\xcazz\x7f\x90\xdd}\x1d\x80\xd9\x81\xbbY&\xc2\x11\xfe\xe6\x93\xe2\xf7\x80\xe4e\xfb47\x0e)\x80pM\xaf8\x86.\x9e\x7fW\xaf\x9e\x17\xae\xeb\x1c\xcb(!_\xb6\x1c\xd8\xe7\xa2\x04\"\xf9\xd3\xda\xd8\xcb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert High Assurance EV Root CA O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert High Assurance EV Root CA O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert High Assurance EV Root CA"
     * Serial: 3553400076410547919724730734378100087
     * MD5 Fingerprint: d4:74:de:57:5c:39:b2:d3:9c:85:83:c5:c0:65:49:8a
     * SHA1 Fingerprint: 5f:b7:ee:06:33:e2:59:db:ad:0c:4c:9a:e6:d3:8f:1a:61:c7:dc:25
     * SHA256 Fingerprint: 74:31:e5:f4:c3:c1:ce:46:90:77:4f:0b:61:e0:54:40:88:3b:a9:a0:1e:d0:0b:a6:ab:d7:80:6e:d3:b1:18:cf
     * -----BEGIN CERTIFICATE-----
     * MIIDxTCCAq2gAwIBAgIQAqxcJmoLQJuPC3nyrkYldzANBgkqhkiG9w0BAQUFADBs
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSswKQYDVQQDEyJEaWdpQ2VydCBIaWdoIEFzc3VyYW5j
     * ZSBFViBSb290IENBMB4XDTA2MTExMDAwMDAwMFoXDTMxMTExMDAwMDAwMFowbDEL
     * MAkGA1UEBhMCVVMxFTATBgNVBAoTDERpZ2lDZXJ0IEluYzEZMBcGA1UECxMQd3d3
     * LmRpZ2ljZXJ0LmNvbTErMCkGA1UEAxMiRGlnaUNlcnQgSGlnaCBBc3N1cmFuY2Ug
     * RVYgUm9vdCBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMbM5XPm
     * +9S75S0tMqbf5YE/yc0lSbZxKsPVlDRnogocsF9ppkCxxLeyj9CYpKlBWTrT3JTW
     * PNt0OKRKzE0lgvdKpVMSOO7zSW1xkX5jtqumX8OkhPhPYlG++MXs2ziS4wblCJEM
     * xChBVfvLWokVfnHoNb9Ncgk9vjo4UFt3MRuNs8ckRZqnrG0AFFoEt7oT61EKmEFB
     * Ik5lYYeBQVCmeVyJ3hlKV9Uu5l0cUyx+mM0aBhakaHPQNAQTXKFx01p8VdteZOE3
     * hzBWBOURtCmAEvF5OYiiAhF8J2a3iLd48soKqDirCmTCv2ZdlYTBoSUeh10aUAsg
     * EsxBu24LUTi4S8sCAwEAAaNjMGEwDgYDVR0PAQH/BAQDAgGGMA8GA1UdEwEB/wQF
     * MAMBAf8wHQYDVR0OBBYEFLE+w2kD+L9HAdSYJhoIAu9jZCvDMB8GA1UdIwQYMBaA
     * FLE+w2kD+L9HAdSYJhoIAu9jZCvDMA0GCSqGSIb3DQEBBQUAA4IBAQAcGgaX3Nec
     * nzyIZgYIVyHbIUf4KmeqvxgydkAQV8GK83rZEWWONfqe/EW1ntlMMUu4kehDLI6z
     * eM7b41N5cdblIZQB2lWHmiRk9opmzN6cN82oNLFpmyPInngiK3BD41VHMWEZ71jF
     * hS9OMPagMRYjyOfiZRYzy78aG6A9+MpeizGLYAiJLQwGXFK3xPkKmNEVX58Svnw2
     * Yzi9RKR/5CYrCsSXaQ3pjOLAEFe4yHYSkVXySGnYvCoCWw9E1CAx2/S6cCZdkGCe
     * vEsXCS+0yx5DaMkHJ8HSXPfqIbloEpw8nL+e/IBcm2PN7EeqJSdnoDfzAIJ9VNep
     * +OkuE6N36B9K
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1+0)\x06\x03U\x04\x03\x13\"DigiCert High Assurance EV Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xc6\xcc\xe5s\xe6\xfb\xd4\xbb\xe5--2\xa6\xdf\xe5\x81?\xc9\xcd%I\xb6q*\xc3\xd5\x944g\xa2\n\x1c\xb0_i\xa6@\xb1\xc4\xb7\xb2\x8f\xd0\x98\xa4\xa9AY:\xd3\xdc\x94\xd6<\xdbt8\xa4J\xccM%\x82\xf7J\xa5S\x128\xee\xf3Imq\x91~c\xb6\xab\xa6_\xc3\xa4\x84\xf8ObQ\xbe\xf8\xc5\xec\xdb8\x92\xe3\x06\xe5\x08\x91\x0c\xc4(AU\xfb\xcbZ\x89\x15~q\xe85\xbfMr\t=\xbe:8P[w1\x1b\x8d\xb3\xc7$E\x9a\xa7\xacm\x00\x14Z\x04\xb7\xba\x13\xebQ\n\x98AA\"Nea\x87\x81AP\xa6y\\\x89\xde\x19JW\xd5.\xe6]\x1cS,~\x98\xcd\x1a\x06\x16\xa4hs\xd04\x04\x13\\\xa1q\xd3Z|U\xdb^d\xe17\x870V\x04\xe5\x11\xb4)\x80\x12\xf1y9\x88\xa2\x02\x11|\'f\xb7\x88\xb7x\xf2\xca\n\xa88\xab\nd\xc2\xbff]\x95\x84\xc1\xa1%\x1e\x87]\x1aP\x0b \x12\xccA\xbbn\x0bQ8\xb8K\xcb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Sonera Class2 CA O=Sonera
     * Subject: CN=Sonera Class2 CA O=Sonera
     * Label: "Sonera Class 2 Root CA"
     * Serial: 29
     * MD5 Fingerprint: a3:ec:75:0f:2e:88:df:fa:48:01:4e:0b:5c:48:6f:fb
     * SHA1 Fingerprint: 37:f7:6d:e6:07:7c:90:c5:b1:3e:93:1a:b7:41:10:b4:f2:e4:9a:27
     * SHA256 Fingerprint: 79:08:b4:03:14:c1:38:10:0b:51:8d:07:35:80:7f:fb:fc:f8:51:8a:00:95:33:71:05:ba:38:6b:15:3d:d9:27
     * -----BEGIN CERTIFICATE-----
     * MIIDIDCCAgigAwIBAgIBHTANBgkqhkiG9w0BAQUFADA5MQswCQYDVQQGEwJGSTEP
     * MA0GA1UEChMGU29uZXJhMRkwFwYDVQQDExBTb25lcmEgQ2xhc3MyIENBMB4XDTAx
     * MDQwNjA3Mjk0MFoXDTIxMDQwNjA3Mjk0MFowOTELMAkGA1UEBhMCRkkxDzANBgNV
     * BAoTBlNvbmVyYTEZMBcGA1UEAxMQU29uZXJhIENsYXNzMiBDQTCCASIwDQYJKoZI
     * hvcNAQEBBQADggEPADCCAQoCggEBAJAXSjWdyvANlsdE+hY3/Ei9vX+ALTU74W+o
     * Z6m/AxxNjG8yR9VBaKQTBME1DJqEQ/xcHf+Js+gXGM2RX/uJ4+q/Tl18GybTdXnt
     * 5oTjV+WtKcT0OijnpXuENmmz/V52vaMtmdOQTiMofRhj8VQ7Jp12W5dCsv+u8E7s
     * 3TmVToMGf+dJQMjFAbJUWmYdPfz56TwKnoG4cPABi+QjVHzIrviQHgCWctRUz2Ej
     * vOr7nQKV0ba5cTppCD8PtOFCx4j1P5iop7oc4HFx71hXgVB6XGt0Rg6DA5jDjqhu
     * 8nYybieDwnPz3BjotJPqdURrBGAgcVeHnfO+oJAjPYok4doh28MCAwEAAaMzMDEw
     * DwYDVR0TAQH/BAUwAwEB/zARBgNVHQ4ECgQISqCqWITTXjwwCwYDVR0PBAQDAgEG
     * MA0GCSqGSIb3DQEBBQUAA4IBAQBazof5FnIVV0sd2ZvnoiYw7JNn39Yt0jSv9zil
     * zqsWuasvfDXLrNAPtEwr/IDva4yRXzZ299uzGxnq9LIR/WFxRL8oszodv7ND6J+/
     * 3DEIcbCdjdY0RzKQxmUk96BKfARzjzlvF4xytb1LyHr4e4PDKE6cCepnP7JnBBvD
     * FNr450kkkdAdavphOe9r5yF1BgfYErQhIHBCcYHaPJo2vqZbDWpsmh+Re/n570K6
     * Tk6ezAyNlNzZRZxe7EJQY670XcSxEtzKO6gunRRaBXW37Ndj4ro1tgQIkejanZz2
     * ZrUYrAqmVCY0M9IbwdR/GjqOC6oybtv8TyWf2TLHllpwrN9M
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02FI1\x0f0\r\x06\x03U\x04\n\x13\x06Sonera1\x190\x17\x06\x03U\x04\x03\x13\x10Sonera Class2 CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\x90\x17J5\x9d\xca\xf0\r\x96\xc7D\xfa\x167\xfcH\xbd\xbd\x7f\x80-5;\xe1o\xa8g\xa9\xbf\x03\x1cM\x8co2G\xd5Ah\xa4\x13\x04\xc15\x0c\x9a\x84C\xfc\\\x1d\xff\x89\xb3\xe8\x17\x18\xcd\x91_\xfb\x89\xe3\xea\xbfN]|\x1b&\xd3uy\xed\xe6\x84\xe3W\xe5\xad)\xc4\xf4:(\xe7\xa5{\x846i\xb3\xfd^v\xbd\xa3-\x99\xd3\x90N#(}\x18c\xf1T;&\x9dv[\x97B\xb2\xff\xae\xf0N\xec\xdd9\x95N\x83\x06\x7f\xe7I@\xc8\xc5\x01\xb2TZf\x1d=\xfc\xf9\xe9<\n\x9e\x81\xb8p\xf0\x01\x8b\xe4#T|\xc8\xae\xf8\x90\x1e\x00\x96r\xd4T\xcfa#\xbc\xea\xfb\x9d\x02\x95\xd1\xb6\xb9q:i\x08?\x0f\xb4\xe1B\xc7\x88\xf5?\x98\xa8\xa7\xba\x1c\xe0qq\xefXW\x81Pz\\ktF\x0e\x83\x03\x98\xc3\x8e\xa8n\xf2v2n\'\x83\xc2s\xf3\xdc\x18\xe8\xb4\x93\xeauDk\x04` qW\x87\x9d\xf3\xbe\xa0\x90#=\x8a$\xe1\xda!\xdb\xc3\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Assured ID Root G2 O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Assured ID Root G2 O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Assured ID Root G2"
     * Serial: 15385348160840213938643033620894905419
     * MD5 Fingerprint: 92:38:b9:f8:63:24:82:65:2c:57:33:e6:fe:81:8f:9d
     * SHA1 Fingerprint: a1:4b:48:d9:43:ee:0a:0e:40:90:4f:3c:e0:a4:c0:91:93:51:5d:3f
     * SHA256 Fingerprint: 7d:05:eb:b6:82:33:9f:8c:94:51:ee:09:4e:eb:fe:fa:79:53:a1:14:ed:b2:f4:49:49:45:2f:ab:7d:2f:c1:85
     * -----BEGIN CERTIFICATE-----
     * MIIDljCCAn6gAwIBAgIQC5McOtY5Z+pnI7/Dr5r0SzANBgkqhkiG9w0BAQsFADBl
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJv
     * b3QgRzIwHhcNMTMwODAxMTIwMDAwWhcNMzgwMTE1MTIwMDAwWjBlMQswCQYDVQQG
     * EwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cuZGlnaWNl
     * cnQuY29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJvb3QgRzIwggEi
     * MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDZ5ygvUj82ckmIkzTz+GoeMVSA
     * n61UQbVH35ao1K+ALbkKz3X9iaV9JPrjIgwrvJUXCzO/GU1BBpAAvQxNEP4Htecc
     * biJVMWWXvdMX0h5i89vqbFCMP4QMls+3ywPgym2hFEwbid3tALBSfK+RbLE4E9Hp
     * EgjAALAcKxHad3A2m67OeYfcgnDmCXRwVWmvo2ifv922ebPynXApVfSr/5Vh88lA
     * bx3RvpO704gqu52/clpWcTs/1PPRCv4o76Pu2ZmvA9OPYLfykqGxvYmJHzDNw6Yu
     * YjOuFgJ3RFrngQo8p0Quebg/BLxcoIfhG69Rjs3sLPr4/m3wOnyqi+RnlTGNAgMB
     * AAGjQjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgGGMB0GA1UdDgQW
     * BBTOw0q5mVXyuNtgv6l+vVa1lzan1jANBgkqhkiG9w0BAQsFAAOCAQEAyqVVjOPI
     * QW5pJ6d1Ee88hjZv0p3GeDgdaZaikmkuOGybfQTUiaWxMTeKySHMq2zNixya1r9I
     * 0jJmwYrA8y8678Dj1JGG0VDjA9tzd29KOVPt3ibHtX2vK0LRdWLjSisCx1BL4Gni
     * lmwORGYQRI+tBev4eaymG+g3NJ1TyWGqolKvSnAWhsI6yLETcDbYz+70CjTVW0z9
     * B5yiutkBclzzTcHdDrEcDcRjvq30FPuJ7KJBDkzMyFdA0G4Dqs0MjomZmWzwPDCv
     * ON9vvKO+KSAnq3T/EyJ43pdSVR6DtVQgA+6uwE9W3jfMw3+qBCe703e4YtsXfJwo
     * IhNzbM8m9Yop5w==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1$0\"\x06\x03U\x04\x03\x13\x1bDigiCert Assured ID Root G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xd9\xe7(/R?6rI\x88\x934\xf3\xf8j\x1e1T\x80\x9f\xadTA\xb5G\xdf\x96\xa8\xd4\xaf\x80-\xb9\n\xcfu\xfd\x89\xa5}$\xfa\xe3\"\x0c+\xbc\x95\x17\x0b3\xbf\x19MA\x06\x90\x00\xbd\x0cM\x10\xfe\x07\xb5\xe7\x1cn\"U1e\x97\xbd\xd3\x17\xd2\x1eb\xf3\xdb\xealP\x8c?\x84\x0c\x96\xcf\xb7\xcb\x03\xe0\xcam\xa1\x14L\x1b\x89\xdd\xed\x00\xb0R|\xaf\x91l\xb18\x13\xd1\xe9\x12\x08\xc0\x00\xb0\x1c+\x11\xdawp6\x9b\xae\xcey\x87\xdc\x82p\xe6\ttpUi\xaf\xa3h\x9f\xbf\xdd\xb6y\xb3\xf2\x9dp)U\xf4\xab\xff\x95a\xf3\xc9@o\x1d\xd1\xbe\x93\xbb\xd3\x88*\xbb\x9d\xbfrZVq;?\xd4\xf3\xd1\n\xfe(\xef\xa3\xee\xd9\x99\xaf\x03\xd3\x8f`\xb7\xf2\x92\xa1\xb1\xbd\x89\x89\x1f0\xcd\xc3\xa6.b3\xae\x16\x02wDZ\xe7\x81\n<\xa7D.y\xb8?\x04\xbc\\\xa0\x87\xe1\x1b\xafQ\x8e\xcd\xec,\xfa\xf8\xfem\xf0:|\xaa\x8b\xe4g\x951\x8d\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Assured ID Root G3 O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Assured ID Root G3 O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Assured ID Root G3"
     * Serial: 15459312981008553731928384953135426796
     * MD5 Fingerprint: 7c:7f:65:31:0c:81:df:8d:ba:3e:99:e2:5c:ad:6e:fb
     * SHA1 Fingerprint: f5:17:a2:4f:9a:48:c6:c9:f8:a2:00:26:9f:dc:0f:48:2c:ab:30:89
     * SHA256 Fingerprint: 7e:37:cb:8b:4c:47:09:0c:ab:36:55:1b:a6:f4:5d:b8:40:68:0f:ba:16:6a:95:2d:b1:00:71:7f:43:05:3f:c2
     * -----BEGIN CERTIFICATE-----
     * MIICRjCCAc2gAwIBAgIQC6Fa+h3foLVJRK/NJKBs7DAKBggqhkjOPQQDAzBlMQsw
     * CQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cu
     * ZGlnaWNlcnQuY29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJvb3Qg
     * RzMwHhcNMTMwODAxMTIwMDAwWhcNMzgwMTE1MTIwMDAwWjBlMQswCQYDVQQGEwJV
     * UzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3d3cuZGlnaWNlcnQu
     * Y29tMSQwIgYDVQQDExtEaWdpQ2VydCBBc3N1cmVkIElEIFJvb3QgRzMwdjAQBgcq
     * hkjOPQIBBgUrgQQAIgNiAAQZ57ysRGXtzbg/WPuNsVepRC0FFfLvC/8QdJ+1YlJf
     * Zn4f5dwbRXkLzMZTCp2NXQLZqVneAlr2lSoOjThKiknGvMYDOAdfVdp+CW7if17Q
     * RSAPWXYQ1qAk8C3eNvJsKTmjQjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/
     * BAQDAgGGMB0GA1UdDgQWBBTL0L2p4ZgFUaFNN6KDec6NHSrkhDAKBggqhkjOPQQD
     * AwNnADBkAjAlpIFFAmsSS3V0T8gj43DydXLefInwz5FyYZ5eEJJZVrmDxxDnOOlY
     * JjZ91eQ0hjkCMHw2U/Aw5WJjOpnitqM7mzT6HtoQknFekROn3aRukswy1vUhZscv
     * 6pZjamVFkpUBtA==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1$0\"\x06\x03U\x04\x03\x13\x1bDigiCert Assured ID Root G3",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x19\xe7\xbc\xacDe\xed\xcd\xb8?X\xfb\x8d\xb1W\xa9D-\x05\x15\xf2\xef\x0b\xff\x10t\x9f\xb5bR_f~\x1f\xe5\xdc\x1bEy\x0b\xcc\xc6S\n\x9d\x8d]\x02\xd9\xa9Y\xde\x02Z\xf6\x95*\x0e\x8d8J\x8aI\xc6\xbc\xc6\x038\x07_U\xda~\tn\xe2\x7f^\xd0E \x0fYv\x10\xd6\xa0$\xf0-\xde6\xf2l)9",
        name_constraints: None
    },

    /*
     * Issuer: CN=OISTE WISeKey Global Root GC CA O=WISeKey OU=OISTE Foundation Endorsed
     * Subject: CN=OISTE WISeKey Global Root GC CA O=WISeKey OU=OISTE Foundation Endorsed
     * Label: "OISTE WISeKey Global Root GC CA"
     * Serial: 44084345621038548146064804565436152554
     * MD5 Fingerprint: a9:d6:b9:2d:2f:93:64:f8:a5:69:ca:91:e9:68:07:23
     * SHA1 Fingerprint: e0:11:84:5e:34:de:be:88:81:b9:9c:f6:16:26:d1:96:1f:c3:b9:31
     * SHA256 Fingerprint: 85:60:f9:1c:36:24:da:ba:95:70:b5:fe:a0:db:e3:6f:f1:1a:83:23:be:94:86:85:4f:b3:f3:4a:55:71:19:8d
     * -----BEGIN CERTIFICATE-----
     * MIICaTCCAe+gAwIBAgIQISpWDK7aDKtARb8roi066jAKBggqhkjOPQQDAzBtMQsw
     * CQYDVQQGEwJDSDEQMA4GA1UEChMHV0lTZUtleTEiMCAGA1UECxMZT0lTVEUgRm91
     * bmRhdGlvbiBFbmRvcnNlZDEoMCYGA1UEAxMfT0lTVEUgV0lTZUtleSBHbG9iYWwg
     * Um9vdCBHQyBDQTAeFw0xNzA1MDkwOTQ4MzRaFw00MjA1MDkwOTU4MzNaMG0xCzAJ
     * BgNVBAYTAkNIMRAwDgYDVQQKEwdXSVNlS2V5MSIwIAYDVQQLExlPSVNURSBGb3Vu
     * ZGF0aW9uIEVuZG9yc2VkMSgwJgYDVQQDEx9PSVNURSBXSVNlS2V5IEdsb2JhbCBS
     * b290IEdDIENBMHYwEAYHKoZIzj0CAQYFK4EEACIDYgAETOlQwMYPchi82PG6s4ni
     * eUqjFqdrVCTbUf/q9Akkwwsin8tqJ4KBDdLArzHkdIJuyiXZjHWd8dvQmqJLIX4W
     * p2OQ0jnUsYd4XxiWD1AbNTcPasbc2RNNpI6QN+a9WzGRo1QwUjAOBgNVHQ8BAf8E
     * BAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUSIcUrOPDnpBgOtfKie7T
     * rYy0UGYwEAYJKwYBBAGCNxUBBAMCAQAwCgYIKoZIzj0EAwMDaAAwZQIwJsdpW9zV
     * 57LnyAyMjMPdeYwbY9XJUpROTYJKcx6ygISpJcBMWm1JKWB4E+J+SOtkAjEA2zQg
     * Mgj/mkkCtojeFK9dbJlxjRo/i9fgojaGHAeCOnZT/cKi7e97sIBPWA9LUzm9
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CH1\x100\x0e\x06\x03U\x04\n\x13\x07WISeKey1\"0 \x06\x03U\x04\x0b\x13\x19OISTE Foundation Endorsed1(0&\x06\x03U\x04\x03\x13\x1fOISTE WISeKey Global Root GC CA",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04L\xe9P\xc0\xc6\x0fr\x18\xbc\xd8\xf1\xba\xb3\x89\xe2yJ\xa3\x16\xa7kT$\xdbQ\xff\xea\xf4\t$\xc3\x0b\"\x9f\xcbj\'\x82\x81\r\xd2\xc0\xaf1\xe4t\x82n\xca%\xd9\x8cu\x9d\xf1\xdb\xd0\x9a\xa2K!~\x16\xa7c\x90\xd29\xd4\xb1\x87x_\x18\x96\x0fP\x1b57\x0fj\xc6\xdc\xd9\x13M\xa4\x8e\x907\xe6\xbd[1\x91",
        name_constraints: None
    },

    /*
     * Issuer: CN=SSL.com Root Certification Authority RSA O=SSL Corporation
     * Subject: CN=SSL.com Root Certification Authority RSA O=SSL Corporation
     * Label: "SSL.com Root Certification Authority RSA"
     * Serial: 8875640296558310041
     * MD5 Fingerprint: 86:69:12:c0:70:f1:ec:ac:ac:c2:d5:bc:a5:5b:a1:29
     * SHA1 Fingerprint: b7:ab:33:08:d1:ea:44:77:ba:14:80:12:5a:6f:bd:a9:36:49:0c:bb
     * SHA256 Fingerprint: 85:66:6a:56:2e:e0:be:5c:e9:25:c1:d8:89:0a:6f:76:a8:7e:c1:6d:4d:7d:5f:29:ea:74:19:cf:20:12:3b:69
     * -----BEGIN CERTIFICATE-----
     * MIIF3TCCA8WgAwIBAgIIeyyb0xaAMpkwDQYJKoZIhvcNAQELBQAwfDELMAkGA1UE
     * BhMCVVMxDjAMBgNVBAgMBVRleGFzMRAwDgYDVQQHDAdIb3VzdG9uMRgwFgYDVQQK
     * DA9TU0wgQ29ycG9yYXRpb24xMTAvBgNVBAMMKFNTTC5jb20gUm9vdCBDZXJ0aWZp
     * Y2F0aW9uIEF1dGhvcml0eSBSU0EwHhcNMTYwMjEyMTczOTM5WhcNNDEwMjEyMTcz
     * OTM5WjB8MQswCQYDVQQGEwJVUzEOMAwGA1UECAwFVGV4YXMxEDAOBgNVBAcMB0hv
     * dXN0b24xGDAWBgNVBAoMD1NTTCBDb3Jwb3JhdGlvbjExMC8GA1UEAwwoU1NMLmNv
     * bSBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5IFJTQTCCAiIwDQYJKoZIhvcN
     * AQEBBQADggIPADCCAgoCggIBAPkP3aMrfcvQKv7sZ4Wm5y4bunfh4/WvpOz6Sl2R
     * xFdHaxh3a3by/ZPkPQ/CFp4LZsNWlJ4Xg4XOVu/yFv0AYvUiCVToZRdOQbngT0aX
     * qhvIuG5iXmmxX9sqAn78bMrzQdjt0Oj8P2FI7bADFB0QDksZ4LtO7IZl/zbzXmcC
     * C52GVWH9ejjt/uIZALdvoVBidXQ8oPrIJZK0bnoix/geoeOy3ZExqysdBP+lSgQ3
     * 6YWkMyv94tZVNHwZpEpox7Ko07fKoZOI68GXvIz5HdkihCR0xwQ9aqkpk8zruFvh
     * /l8lqjRYyMEjVJ0bmBHDOJx+PYZspQ9AhnwC9FwCTyjLrnGfDzrIM/4RJTXq/LrF
     * YD3ZfBjVsqnTdXgDciLKOsMf7yzlLqn6niy2UUb9rwPW6mBo6oUWNmuF6R7As93E
     * JNyAKoFBbZQ+yODJgUEAnl6/f8UImKIYLEJAs/lvOCdLToD0PYFH4Ih86hzOtXVc
     * US4cK38acijnALXRdMbX5J+tB5O2UzU1/Dfkw/ZdFr4hc96SCvigY2q8lpJqPvi8
     * ZVWb3vUNiSYE/CUapiVpy8JtynziWV+XrOvvLsi81xtZPCvM8hnIk2snYxnP/Okm
     * +Mpxm3+T/jRnhE6Z6/yzeAkzcLpmpnbtG3PrGqUNxCITIJRWCk4sbE6x/c+cCbqi
     * M+2HAgMBAAGjYzBhMB0GA1UdDgQWBBTdBAkHovV6fVJTEpKV7jiAJQ2mWTAPBgNV
     * HRMBAf8EBTADAQH/MB8GA1UdIwQYMBaAFN0ECQei9Xp9UlMSkpXuOIAlDaZZMA4G
     * A1UdDwEB/wQEAwIBhjANBgkqhkiG9w0BAQsFAAOCAgEAIBgRlCn7Jp0cHh5wYfGV
     * cpNxJK1ok1iOMq8bs3AD/CUrdIWQPXhq9LmLpZc7tRiRux6n+UBbkflVma8eEdBc
     * Hadm47GUBwwyOabqG7B52B2ccETjit3E+ZUfijhDPwGFpUenPUayvOUiaPd7nNgs
     * PgohyC0zrL/FgZkxdMF1ccW+sfAjRfSda/wZY52jvATGGAslu1OJD7OAUN5F7kR/
     * q5R4ZJjT9ijdh9hwZXT7DrkT66cPYakylszeu+1jTBi7qUD3oFRuIIhxdRjqerQ0
     * cuAjJ3dctpDqhiVAq+8zD8ufgr6iIPv2tS0a5sKFsXQP+8hlAqRSAUfdSSLBv9jr
     * a6x+3uxjMxW3IwiPxg+NQVrdjsW5j+VFP3jbutIbQLH+cU0/4IGiul607BXgk90I
     * H37hVZkLId6Tngr75qNJvTYw/ud3sqB1l7UtgYgXZSD32pAAn8lSzDLKNXz1PQ/Y
     * K9f1JmzJBjSWFupwWRoyeXkLtoh/D1JIPb9s2KJELtFOt3JY04kTlf5Eq/jXixtu
     * nLwsoFvVagCvXzfh1foQC5ichucmj87w7G6KVwuA406ywKBjYZC6VWg3dGq2ktuf
     * oYYitmUnDuy2n0Jg5GfCtdpBC8TTi2EbvPofkSvXRAdeuims2cXp71NIWuuA8ShY
     * Ic2wBlX7Jz9TkHCpBB5XJ7k=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0e0\x0c\x06\x03U\x04\x08\x0c\x05Texas1\x100\x0e\x06\x03U\x04\x07\x0c\x07Houston1\x180\x16\x06\x03U\x04\n\x0c\x0fSSL Corporation110/\x06\x03U\x04\x03\x0c(SSL.com Root Certification Authority RSA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xf9\x0f\xdd\xa3+}\xcb\xd0*\xfe\xecg\x85\xa6\xe7.\x1b\xbaw\xe1\xe3\xf5\xaf\xa4\xec\xfaJ]\x91\xc4WGk\x18wkv\xf2\xfd\x93\xe4=\x0f\xc2\x16\x9e\x0bf\xc3V\x94\x9e\x17\x83\x85\xceV\xef\xf2\x16\xfd\x00b\xf5\"\tT\xe8e\x17NA\xb9\xe0OF\x97\xaa\x1b\xc8\xb8nb^i\xb1_\xdb*\x02~\xfcl\xca\xf3A\xd8\xed\xd0\xe8\xfc?aH\xed\xb0\x03\x14\x1d\x10\x0eK\x19\xe0\xbbN\xec\x86e\xff6\xf3^g\x02\x0b\x9d\x86Ua\xfdz8\xed\xfe\xe2\x19\x00\xb7o\xa1Pbut<\xa0\xfa\xc8%\x92\xb4nz\"\xc7\xf8\x1e\xa1\xe3\xb2\xdd\x911\xab+\x1d\x04\xff\xa5J\x047\xe9\x85\xa43+\xfd\xe2\xd6U4|\x19\xa4Jh\xc7\xb2\xa8\xd3\xb7\xca\xa1\x93\x88\xeb\xc1\x97\xbc\x8c\xf9\x1d\xd9\"\x84$t\xc7\x04=j\xa9)\x93\xcc\xeb\xb8[\xe1\xfe_%\xaa4X\xc8\xc1#T\x9d\x1b\x98\x11\xc38\x9c~=\x86l\xa5\x0f@\x86|\x02\xf4\\\x02O(\xcb\xaeq\x9f\x0f:\xc83\xfe\x11%5\xea\xfc\xba\xc5`=\xd9|\x18\xd5\xb2\xa9\xd3ux\x03r\"\xca:\xc3\x1f\xef,\xe5.\xa9\xfa\x9e,\xb6QF\xfd\xaf\x03\xd6\xea`h\xea\x85\x166k\x85\xe9\x1e\xc0\xb3\xdd\xc4$\xdc\x80*\x81Am\x94>\xc8\xe0\xc9\x81A\x00\x9e^\xbf\x7f\xc5\x08\x98\xa2\x18,B@\xb3\xf9o8\'KN\x80\xf4=\x81G\xe0\x88|\xea\x1c\xce\xb5u\\Q.\x1c+\x7f\x1ar(\xe7\x00\xb5\xd1t\xc6\xd7\xe4\x9f\xad\x07\x93\xb6S55\xfc7\xe4\xc3\xf6]\x16\xbe!s\xde\x92\n\xf8\xa0cj\xbc\x96\x92j>\xf8\xbceU\x9b\xde\xf5\r\x89&\x04\xfc%\x1a\xa6%i\xcb\xc2m\xca|\xe2Y_\x97\xac\xeb\xef.\xc8\xbc\xd7\x1bY<+\xcc\xf2\x19\xc8\x93k\'c\x19\xcf\xfc\xe9&\xf8\xcaq\x9b\x7f\x93\xfe4g\x84N\x99\xeb\xfc\xb3x\t3p\xbaf\xa6v\xed\x1bs\xeb\x1a\xa5\r\xc4\"\x13 \x94V\nN,lN\xb1\xfd\xcf\x9c\t\xba\xa23\xed\x87\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root CA 2 O=QuoVadis Limited
     * Subject: CN=QuoVadis Root CA 2 O=QuoVadis Limited
     * Label: "QuoVadis Root CA 2"
     * Serial: 1289
     * MD5 Fingerprint: 5e:39:7b:dd:f8:ba:ec:82:e9:ac:62:ba:0c:54:00:2b
     * SHA1 Fingerprint: ca:3a:fb:cf:12:40:36:4b:44:b2:16:20:88:80:48:39:19:93:7c:f7
     * SHA256 Fingerprint: 85:a0:dd:7d:d7:20:ad:b7:ff:05:f8:3d:54:2b:20:9d:c7:ff:45:28:f7:d6:77:b1:83:89:fe:a5:e5:c4:9e:86
     * -----BEGIN CERTIFICATE-----
     * MIIFtzCCA5+gAwIBAgICBQkwDQYJKoZIhvcNAQEFBQAwRTELMAkGA1UEBhMCQk0x
     * GTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxGzAZBgNVBAMTElF1b1ZhZGlzIFJv
     * b3QgQ0EgMjAeFw0wNjExMjQxODI3MDBaFw0zMTExMjQxODIzMzNaMEUxCzAJBgNV
     * BAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBMaW1pdGVkMRswGQYDVQQDExJRdW9W
     * YWRpcyBSb290IENBIDIwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCa
     * GMpLlA0ALa8DKYrwD4HIrkwZhR0In6spRIXzL4GtMh6QRr+jhiYaHv5+HBg6XJxg
     * Fyo6dIMzMH1hVBHL7avg5tKifvVrbxi3Cgst/ek+7wrGsxDp3MJGF/hd/aTa/55J
     * WpzmM+Yklvc/ulsrHHo1wtZn/qtmUIttKGAr79dgw8eTvI02kfN/+NsRE8Scd3bB
     * rrcCaoF6qUWD4gXmuVbBlDePSHFjIuwXZQeVikvfj8ZaCuWw419eaxGrDPmF60Tp
     * +ARz8un+XJiM9XOva7R+zdRcAitMOeGylZUtQofX1bOQQ7dsE/He3fbE+Ik/0XX1
     * ksOR1YqI0JDs3G3eicJlcZaLDQP9nL9bFqyS2+r+eXyt66/3FsvbzSUr5R/7mp/i
     * Ucw6UwxI5g69ybR2BlLmEROFcmMDBOAENisgGQLodKcftslWZvB1JdxnwQ5hYIiz
     * PtGo/KPaHbDRsSNU30R2be1B2MGyIrZTHN81Hdyhdyox5C315eXbyOD/5YDXC2Og
     * /zOhD7osFRXql7PSorW+8oyWHhqPHWykYTe5hnMz15eWniN9gqRMgeKh0bpnX5UH
     * oycR7hYQe7xFSkyyBNKr79X9DFHOUGoIMfmR2gyPZFwDwzqLID9ujWc9Otb+fVuI
     * yV77zGHcizN300QyNQliBJIWENieJ0f7OyHj+OsdWwIDAQABo4GwMIGtMA8GA1Ud
     * EwEB/wQFMAMBAf8wCwYDVR0PBAQDAgEGMB0GA1UdDgQWBBQahGK8SEwzJQTU7tD2
     * A8QZRtGUazBuBgNVHSMEZzBlgBQahGK8SEwzJQTU7tD2A8QZRtGUa6FJpEcwRTEL
     * MAkGA1UEBhMCQk0xGTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxGzAZBgNVBAMT
     * ElF1b1ZhZGlzIFJvb3QgQ0EgMoICBQkwDQYJKoZIhvcNAQEFBQADggIBAD4KFk2f
     * BluornFdLwUvZ+YTRYPENvbzwCYMDbVHZF34tHLJRqUDGCdViXh9duqWNIAXINzn
     * g/iN/Ae42l9NLmeyhP3ZRPx3UIHmfLTJDQtyU/h2BwdBR5YM++CCJpNVjP4iH2Bl
     * fF/nJrP3MpCYUNQ3cVX2kiF495V5+vgtJodmVjB3pjd4M1IQWK4/YY7yarHvGH5K
     * WWPKjaJW1acvvFYfzznB4vsKqBUsfU16Y8Zsl0Q80m/DShcK+JDSV6IZUaUtl0Ha
     * B0+pUNqQjZRG4T7wlP0QADj1O+hA4bRuVhogzG9Yje0uRY/W6ZM/57Es3zrWIozc
     * hLsib9D45MY56QSIPMO661V6bYCZJPVsAfv4l7CUW+v90m/xd2gNNWQjrLhVoQPR
     * TUIZ3Ph1WVaj+ahJefivDrkRoHy3au000LYmYjgahwz46P0u05B/B5EqHdZ+XIWD
     * mbA4CD/pXvk1B+TJYm5Xf6dQlfe6yJvmjqIBxdZmv3lh8zwc4bmCXF2gw+nYSL0Z
     * ohEUGW6yhhtoPkg3Goi3XZZenMfvJ2II4pEZXNLxId26F0KCl3GBUzGpn/Z9Yr9y
     * 4aOTHcyKJloJONDO1w2AFrR4pTqHTI2KpdVGl/IsELm8VCLAAVBpQ570su9t+Oza
     * 8eOx79+Rj1QqCyXBJhnEUhAFZdWCEOrCMc0u
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1\x1b0\x19\x06\x03U\x04\x03\x13\x12QuoVadis Root CA 2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x9a\x18\xcaK\x94\r\x00-\xaf\x03)\x8a\xf0\x0f\x81\xc8\xaeL\x19\x85\x1d\x08\x9f\xab)D\x85\xf3/\x81\xad2\x1e\x90F\xbf\xa3\x86&\x1a\x1e\xfe~\x1c\x18:\\\x9c`\x17*:t\x8330}aT\x11\xcb\xed\xab\xe0\xe6\xd2\xa2~\xf5ko\x18\xb7\n\x0b-\xfd\xe9>\xef\n\xc6\xb3\x10\xe9\xdc\xc2F\x17\xf8]\xfd\xa4\xda\xff\x9eIZ\x9c\xe63\xe6$\x96\xf7?\xba[+\x1cz5\xc2\xd6g\xfe\xabfP\x8bm(`+\xef\xd7`\xc3\xc7\x93\xbc\x8d6\x91\xf3\x7f\xf8\xdb\x11\x13\xc4\x9cwv\xc1\xae\xb7\x02j\x81z\xa9E\x83\xe2\x05\xe6\xb9V\xc1\x947\x8fHqc\"\xec\x17e\x07\x95\x8aK\xdf\x8f\xc6Z\n\xe5\xb0\xe3_^k\x11\xab\x0c\xf9\x85\xebD\xe9\xf8\x04s\xf2\xe9\xfe\\\x98\x8c\xf5s\xafk\xb4~\xcd\xd4\\\x02+L9\xe1\xb2\x95\x95-B\x87\xd7\xd5\xb3\x90C\xb7l\x13\xf1\xde\xdd\xf6\xc4\xf8\x89?\xd1u\xf5\x92\xc3\x91\xd5\x8a\x88\xd0\x90\xec\xdcm\xde\x89\xc2eq\x96\x8b\r\x03\xfd\x9c\xbf[\x16\xac\x92\xdb\xea\xfey|\xad\xeb\xaf\xf7\x16\xcb\xdb\xcd%+\xe5\x1f\xfb\x9a\x9f\xe2Q\xcc:S\x0cH\xe6\x0e\xbd\xc9\xb4v\x06R\xe6\x11\x13\x85rc\x03\x04\xe0\x046+ \x19\x02\xe8t\xa7\x1f\xb6\xc9Vf\xf0u%\xdcg\xc1\x0ea`\x88\xb3>\xd1\xa8\xfc\xa3\xda\x1d\xb0\xd1\xb1#T\xdfDvm\xedA\xd8\xc1\xb2\"\xb6S\x1c\xdf5\x1d\xdc\xa1w*1\xe4-\xf5\xe5\xe5\xdb\xc8\xe0\xff\xe5\x80\xd7\x0bc\xa0\xff3\xa1\x0f\xba,\x15\x15\xea\x97\xb3\xd2\xa2\xb5\xbe\xf2\x8c\x96\x1e\x1a\x8f\x1dl\xa4a7\xb9\x86s3\xd7\x97\x96\x9e#}\x82\xa4L\x81\xe2\xa1\xd1\xbag_\x95\x07\xa3\'\x11\xee\x16\x10{\xbcEJL\xb2\x04\xd2\xab\xef\xd5\xfd\x0cQ\xcePj\x081\xf9\x91\xda\x0c\x8fd\\\x03\xc3:\x8b ?n\x8dg=:\xd6\xfe}[\x88\xc9^\xfb\xcca\xdc\x8b3w\xd3D25\tb\x04\x92\x16\x10\xd8\x9e\'G\xfb;!\xe3\xf8\xeb\x1d[\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=emSign ECC Root CA - G3 O=eMudhra Technologies Limited OU=emSign PKI
     * Subject: CN=emSign ECC Root CA - G3 O=eMudhra Technologies Limited OU=emSign PKI
     * Label: "emSign ECC Root CA - G3"
     * Serial: 287880440101571086945156
     * MD5 Fingerprint: ce:0b:72:d1:9f:88:8e:d0:50:03:e8:e3:b8:8b:67:40
     * SHA1 Fingerprint: 30:43:fa:4f:f2:57:dc:a0:c3:80:ee:2e:58:ea:78:b2:3f:e6:bb:c1
     * SHA256 Fingerprint: 86:a1:ec:ba:08:9c:4a:8d:3b:be:27:34:c6:12:ba:34:1d:81:3e:04:3c:f9:e8:a8:62:cd:5c:57:a3:6b:be:6b
     * -----BEGIN CERTIFICATE-----
     * MIICTjCCAdOgAwIBAgIKPPYHqWhwDtqLhDAKBggqhkjOPQQDAzBrMQswCQYDVQQG
     * EwJJTjETMBEGA1UECxMKZW1TaWduIFBLSTElMCMGA1UEChMcZU11ZGhyYSBUZWNo
     * bm9sb2dpZXMgTGltaXRlZDEgMB4GA1UEAxMXZW1TaWduIEVDQyBSb290IENBIC0g
     * RzMwHhcNMTgwMjE4MTgzMDAwWhcNNDMwMjE4MTgzMDAwWjBrMQswCQYDVQQGEwJJ
     * TjETMBEGA1UECxMKZW1TaWduIFBLSTElMCMGA1UEChMcZU11ZGhyYSBUZWNobm9s
     * b2dpZXMgTGltaXRlZDEgMB4GA1UEAxMXZW1TaWduIEVDQyBSb290IENBIC0gRzMw
     * djAQBgcqhkjOPQIBBgUrgQQAIgNiAAQjpQy4LRL1KPOxst3iAhKAnjlfSU2fySU0
     * WXTsuwYc58Byr+iuL+FBVIcUqEqy6HyC5ltqtdyzdc6LBtCGI79G1Y4PPwT01xyS
     * fvalY8L1X44uT6EYGQIrMgqCZH0Wk9GjQjBAMB0GA1UdDgQWBBR8XQKEE9TMipuB
     * zhccLikenEhjQjAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAKBggq
     * hkjOPQQDAwNpADBmAjEAvvNhzwIQHWSVB7gYboiFBS+DCBeQyh+KTOgNG3qxrdWB
     * CUfvO6wIBHxcmbHtRwfSAjEAnbpV/KlK6O3t5nYBQnvI+GDZjVGLVTv7jHvrZQnD
     * +JbNR6iC8hZVdyR+EhCVBCyj
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02IN1\x130\x11\x06\x03U\x04\x0b\x13\nemSign PKI1%0#\x06\x03U\x04\n\x13\x1ceMudhra Technologies Limited1 0\x1e\x06\x03U\x04\x03\x13\x17emSign ECC Root CA - G3",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04#\xa5\x0c\xb8-\x12\xf5(\xf3\xb1\xb2\xdd\xe2\x02\x12\x80\x9e9_IM\x9f\xc9%4Yt\xec\xbb\x06\x1c\xe7\xc0r\xaf\xe8\xae/\xe1AT\x87\x14\xa8J\xb2\xe8|\x82\xe6[j\xb5\xdc\xb3u\xce\x8b\x06\xd0\x86#\xbfF\xd5\x8e\x0f?\x04\xf4\xd7\x1c\x92~\xf6\xa5c\xc2\xf5_\x8e.O\xa1\x18\x19\x02+2\n\x82d}\x16\x93\xd1",
        name_constraints: None
    },

    /*
     * Issuer: CN=EC-ACC O=Agencia Catalana de Certificacio (NIF Q-0801176-I) OU=Serveis Publics de Certificacio/Vegeu https://www.catcert.net/verarrel (c)03/Jerarquia Entitats de Certificacio Catalanes
     * Subject: CN=EC-ACC O=Agencia Catalana de Certificacio (NIF Q-0801176-I) OU=Serveis Publics de Certificacio/Vegeu https://www.catcert.net/verarrel (c)03/Jerarquia Entitats de Certificacio Catalanes
     * Label: "EC-ACC"
     * Serial: -23701579247955709139626555126524820479
     * MD5 Fingerprint: eb:f5:9d:29:0d:61:f9:42:1f:7c:c2:ba:6d:e3:15:09
     * SHA1 Fingerprint: 28:90:3a:63:5b:52:80:fa:e6:77:4c:0b:6d:a7:d6:ba:a6:4a:f2:e8
     * SHA256 Fingerprint: 88:49:7f:01:60:2f:31:54:24:6a:e2:8c:4d:5a:ef:10:f1:d8:7e:bb:76:62:6f:4a:e0:b7:f9:5b:a7:96:87:99
     * -----BEGIN CERTIFICATE-----
     * MIIFVjCCBD6gAwIBAgIQ7is969Qh3hSoYqwE893EATANBgkqhkiG9w0BAQUFADCB
     * 8zELMAkGA1UEBhMCRVMxOzA5BgNVBAoTMkFnZW5jaWEgQ2F0YWxhbmEgZGUgQ2Vy
     * dGlmaWNhY2lvIChOSUYgUS0wODAxMTc2LUkpMSgwJgYDVQQLEx9TZXJ2ZWlzIFB1
     * YmxpY3MgZGUgQ2VydGlmaWNhY2lvMTUwMwYDVQQLEyxWZWdldSBodHRwczovL3d3
     * dy5jYXRjZXJ0Lm5ldC92ZXJhcnJlbCAoYykwMzE1MDMGA1UECxMsSmVyYXJxdWlh
     * IEVudGl0YXRzIGRlIENlcnRpZmljYWNpbyBDYXRhbGFuZXMxDzANBgNVBAMTBkVD
     * LUFDQzAeFw0wMzAxMDcyMzAwMDBaFw0zMTAxMDcyMjU5NTlaMIHzMQswCQYDVQQG
     * EwJFUzE7MDkGA1UEChMyQWdlbmNpYSBDYXRhbGFuYSBkZSBDZXJ0aWZpY2FjaW8g
     * KE5JRiBRLTA4MDExNzYtSSkxKDAmBgNVBAsTH1NlcnZlaXMgUHVibGljcyBkZSBD
     * ZXJ0aWZpY2FjaW8xNTAzBgNVBAsTLFZlZ2V1IGh0dHBzOi8vd3d3LmNhdGNlcnQu
     * bmV0L3ZlcmFycmVsIChjKTAzMTUwMwYDVQQLEyxKZXJhcnF1aWEgRW50aXRhdHMg
     * ZGUgQ2VydGlmaWNhY2lvIENhdGFsYW5lczEPMA0GA1UEAxMGRUMtQUNDMIIBIjAN
     * BgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAsyLHT+KXQpWIR4NA9h0X84NzJB5R
     * 85iKw5K4/0CQBXCHYMkAqbWUZRkiFRfCQ2xmRJoNBD45b6VLeqpjt4pEndljkYRm
     * 4CgPukLjbo73FCeTae6RDqNfDrHrZqJyTxIThmV6PttPB/SnCWDaOkKZx7J/sxaV
     * HMf5NLWUhdWZXqBIoH7nF2W4onW4HvPlQn2v7fOKSGRdghST2MDk/7NQcvJ29rNd
     * QlB50JQ+awwAvthrDk4q7D7SzIKiGGUzE3eeml0aE9jD2z3Il3rucO2n5nzbcc8t
     * lGLfbdb1OL4/pYUKGbio2Al1QnDE6u/LDsg0qBIimAy4E5S2S+zw0JDnJwIDAQAB
     * o4HjMIHgMB0GA1UdEQQWMBSBEmVjX2FjY0BjYXRjZXJ0Lm5ldDAPBgNVHRMBAf8E
     * BTADAQH/MA4GA1UdDwEB/wQEAwIBBjAdBgNVHQ4EFgQUoMOLRKo3pUW/l4Ba0fF4
     * opvpXY0wfwYDVR0gBHgwdjB0BgsrBgEEAfV4AQMBCjBlMCwGCCsGAQUFBwIBFiBo
     * dHRwczovL3d3dy5jYXRjZXJ0Lm5ldC92ZXJhcnJlbDA1BggrBgEFBQcCAjApGidW
     * ZWdldSBodHRwczovL3d3dy5jYXRjZXJ0Lm5ldC92ZXJhcnJlbCAwDQYJKoZIhvcN
     * AQEFBQADggEBAKBIW4IB9k1IuDlVNZyAelOZ1Vr/sXE7zDkJlF7W2u++AVtd0x7Y
     * /X1PzaBB4DSTv8vihpw3kpBWHNzrKQXlxJ7HNd+KDM3FIUPpqojlNcAZQmNaAl6k
     * SBg6hW/cnbw/nZzBh7h6YQjpdwt/cKt63dmXLGQehb+8dJahw3oS7AwaboMMPOhy
     * Rp/7SNVel+axofjk70YllJyJ22k4vuxcDlbHZVHlUIiIv0LVKz3l+bqeLrPK9HOS
     * Agu+TGbrIP65y7WZf+a2E/rKS03Z7lNGBjvGTq2TWoF+bCpLagVFjPIhpDGQh2xl
     * nJ2lYJU6Un/10asIbvPuW/mIPX64b24D5EI=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02ES1;09\x06\x03U\x04\n\x132Agencia Catalana de Certificacio (NIF Q-0801176-I)1(0&\x06\x03U\x04\x0b\x13\x1fServeis Publics de Certificacio1503\x06\x03U\x04\x0b\x13,Vegeu https://www.catcert.net/verarrel (c)031503\x06\x03U\x04\x0b\x13,Jerarquia Entitats de Certificacio Catalanes1\x0f0\r\x06\x03U\x04\x03\x13\x06EC-ACC",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb3\"\xc7O\xe2\x97B\x95\x88G\x83@\xf6\x1d\x17\xf3\x83s$\x1eQ\xf3\x98\x8a\xc3\x92\xb8\xff@\x90\x05p\x87`\xc9\x00\xa9\xb5\x94e\x19\"\x15\x17\xc2ClfD\x9a\r\x04>9o\xa5Kz\xaac\xb7\x8aD\x9d\xd9c\x91\x84f\xe0(\x0f\xbaB\xe3n\x8e\xf7\x14\'\x93i\xee\x91\x0e\xa3_\x0e\xb1\xebf\xa2rO\x12\x13\x86ez>\xdbO\x07\xf4\xa7\t`\xda:B\x99\xc7\xb2\x7f\xb3\x16\x95\x1c\xc7\xf94\xb5\x94\x85\xd5\x99^\xa0H\xa0~\xe7\x17e\xb8\xa2u\xb8\x1e\xf3\xe5B}\xaf\xed\xf3\x8aHd]\x82\x14\x93\xd8\xc0\xe4\xff\xb3Pr\xf2v\xf6\xb3]BPy\xd0\x94>k\x0c\x00\xbe\xd8k\x0eN*\xec>\xd2\xcc\x82\xa2\x18e3\x13w\x9e\x9a]\x1a\x13\xd8\xc3\xdb=\xc8\x97z\xeep\xed\xa7\xe6|\xdbq\xcf-\x94b\xdfm\xd6\xf58\xbe?\xa5\x85\n\x19\xb8\xa8\xd8\tuBp\xc4\xea\xef\xcb\x0e\xc84\xa8\x12\"\x98\x0c\xb8\x13\x94\xb6K\xec\xf0\xd0\x90\xe7\'\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root CA 3 G3 O=QuoVadis Limited
     * Subject: CN=QuoVadis Root CA 3 G3 O=QuoVadis Limited
     * Label: "QuoVadis Root CA 3 G3"
     * Serial: 268090761170461462463995952157327242137089239581
     * MD5 Fingerprint: df:7d:b9:ad:54:6f:68:a1:df:89:57:03:97:43:b0:d7
     * SHA1 Fingerprint: 48:12:bd:92:3c:a8:c4:39:06:e7:30:6d:27:96:e6:a4:cf:22:2e:7d
     * SHA256 Fingerprint: 88:ef:81:de:20:2e:b0:18:45:2e:43:f8:64:72:5c:ea:5f:bd:1f:c2:d9:d2:05:73:07:09:c5:d8:b8:69:0f:46
     * -----BEGIN CERTIFICATE-----
     * MIIFYDCCA0igAwIBAgIULvWbAiin23r/1aOp7r0DoM8Sah0wDQYJKoZIhvcNAQEL
     * BQAwSDELMAkGA1UEBhMCQk0xGTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxHjAc
     * BgNVBAMTFVF1b1ZhZGlzIFJvb3QgQ0EgMyBHMzAeFw0xMjAxMTIyMDI2MzJaFw00
     * MjAxMTIyMDI2MzJaMEgxCzAJBgNVBAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBM
     * aW1pdGVkMR4wHAYDVQQDExVRdW9WYWRpcyBSb290IENBIDMgRzMwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQCzyw4QZ47qFJenMioKVjZ/aEzHs286IxSR
     * /xl/pcqs7rN2nXrpixurazHb+gtTTK/FpRp5PIpM/6zfJd5O2YIyC0TeytuMrKNu
     * FoM7pmRLMon7FhY4futD4tN0SsJiCnMK3UmzV9KwCoWdcTzeo8vAMvMBOSBDGzXR
     * U7Ox7sWTaYI+FrUoRqHe6okJ7UO4BUaKhvVZR74bbwEhELn9qdIoyhA5CcoTNs+c
     * ra1AdHkrAj80//ogaX3T7mH1urPnMNA3I4ZyYUUpSFlob3emLoG+B01vr87ERROR
     * FHAGjx+f+IdpsQ7vw4kZ6+ocYfx6bIrc1gMLnia6Et3UVDmrJqMz6nWB2i3ND0/k
     * A9HvFZcba5DFApCTZgIhsUfei5pKgLlVj7WiL8DWM2fafsSntARE60f75li59wzw
     * eyuxwHApw0BiLTtIadwjPEjrewl5qW3aqDCYz4ByA4imW0aucnl8CAMhZa634Ryl
     * sSqiMd5mBPfAdOhx3v89WcyWJhKLhZVXGqtrdQtEPREoPHtht+KPZ0/l7DxMYIBp
     * VzgeAVuNVejH38DMdyM0SXV89pgR6y3e7UEuFAUCf+D+IOs15xGsIs5XPd7JMG0Q
     * A4XN8f+MFrXBsj6IbGB/kE+V9/YtrQE5BwT6dYB9v0lQ7e/JxHwc64B+27bQ3RP+
     * ydOc17KXqQIDAQABo0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIB
     * BjAdBgNVHQ4EFgQUxhfQvKjqAkPyGwaZXSuQILnXnOQwDQYJKoZIhvcNAQELBQAD
     * ggIBADRh2Va1EodVTd2jNTFGu6QHcrxfYWLopfsLN7E8trP6KZ1/AvWkyaiTt3px
     * KGmPc+FSkNrVvjrlt3ZqVoAh313m6Tqe5T72omnHKgqwGEfcIHB9UqM+WXzBusnI
     * FUBhynLWcKzSt/Ac5IYp8M7vaGPQtSCKFWGafoaYtMnCdvvMujAWzKNhxnQT5Wvv
     * oxXqA/4Ti2Tk08HS6IT7SdEQTXlm66r99I0xHnAUrdzeZxNMgRVhvLfZkXdxGYFg
     * u/BYpbWcC/ePIlUnwEsBbTuZDdQdm2NnL9DuDcpmvJRPpq3t/O5jrFc/ZSXPsoaP
     * 0Aj/uHYUbt7lJ+yreLVTubY/6CD50qi+YUbKh4yE8/nxoGibIh6BJpsQBJFxwAYf
     * 3KDTuVan45gtf4Od34wrnDKOMpTwATwiKp9Dwi7DmDkHOHv8XgBCH/MyJnmDhPbl
     * 8MFREsALHgQjDFSlTC9JxUrRtm5gDWv8a4uFJGS3iQ6rJUdbPM9+Sb3H6QrG2vd+
     * DhcI00iX0HGS8A85PjRqHH3Y8iKuu2n0M7SmSFXRDw4m6Oy2Cy2nhTXN/VnIn9HN
     * PlopNLk9hM6xZdRZkZFWdSHBd575euFgndOtBBj0fOtek49TSiIp+EgrPk2GrFt/
     * ywaZWWDYWGWVjUTR939+J399roD1B0y2PpxxVJkES/1Y+Zj0
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1\x1e0\x1c\x06\x03U\x04\x03\x13\x15QuoVadis Root CA 3 G3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb3\xcb\x0e\x10g\x8e\xea\x14\x97\xa72*\nV6\x7fhL\xc7\xb3o:#\x14\x91\xff\x19\x7f\xa5\xca\xac\xee\xb3v\x9dz\xe9\x8b\x1b\xabk1\xdb\xfa\x0bSL\xaf\xc5\xa5\x1ay<\x8aL\xff\xac\xdf%\xdeN\xd9\x822\x0bD\xde\xca\xdb\x8c\xac\xa3n\x16\x83;\xa6dK2\x89\xfb\x16\x168~\xebC\xe2\xd3tJ\xc2b\ns\n\xddI\xb3W\xd2\xb0\n\x85\x9dq<\xde\xa3\xcb\xc02\xf3\x019 C\x1b5\xd1S\xb3\xb1\xee\xc5\x93i\x82>\x16\xb5(F\xa1\xde\xea\x89\t\xedC\xb8\x05F\x8a\x86\xf5YG\xbe\x1bo\x01!\x10\xb9\xfd\xa9\xd2(\xca\x109\t\xca\x136\xcf\x9c\xad\xad@ty+\x02?4\xff\xfa i}\xd3\xeea\xf5\xba\xb3\xe70\xd07#\x86raE)HYhow\xa6.\x81\xbe\x07Mo\xaf\xce\xc4E\x13\x91\x14p\x06\x8f\x1f\x9f\xf8\x87i\xb1\x0e\xef\xc3\x89\x19\xeb\xea\x1ca\xfczl\x8a\xdc\xd6\x03\x0b\x9e&\xba\x12\xdd\xd4T9\xab&\xa33\xeau\x81\xda-\xcd\x0fO\xe4\x03\xd1\xef\x15\x97\x1bk\x90\xc5\x02\x90\x93f\x02!\xb1G\xde\x8b\x9aJ\x80\xb9U\x8f\xb5\xa2/\xc0\xd63g\xda~\xc4\xa7\xb4\x04D\xebG\xfb\xe6X\xb9\xf7\x0c\xf0{+\xb1\xc0p)\xc3@b-;Hi\xdc#<H\xeb{\ty\xa9m\xda\xa80\x98\xcf\x80r\x03\x88\xa6[F\xaery|\x08\x03!e\xae\xb7\xe1\x1c\xa5\xb1*\xa21\xdef\x04\xf7\xc0t\xe8q\xde\xff=Y\xcc\x96&\x12\x8b\x85\x95W\x1a\xabku\x0bD=\x11(<{a\xb7\xe2\x8fgO\xe5\xec<L`\x80iW8\x1e\x01[\x8dU\xe8\xc7\xdf\xc0\xccw#4Iu|\xf6\x98\x11\xeb-\xde\xedA.\x14\x05\x02\x7f\xe0\xfe \xeb5\xe7\x11\xac\"\xceW=\xde\xc90m\x10\x03\x85\xcd\xf1\xff\x8c\x16\xb5\xc1\xb2>\x88l`\x7f\x90O\x95\xf7\xf6-\xad\x019\x07\x04\xfau\x80}\xbfIP\xed\xef\xc9\xc4|\x1c\xeb\x80~\xdb\xb6\xd0\xdd\x13\xfe\xc9\xd3\x9c\xd7\xb2\x97\xa9\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=NAVER Global Root Certification Authority O=NAVER BUSINESS PLATFORM Corp.
     * Subject: CN=NAVER Global Root Certification Authority O=NAVER BUSINESS PLATFORM Corp.
     * Label: "NAVER Global Root Certification Authority"
     * Serial: 9013692873798656336226253319739695165984492813
     * MD5 Fingerprint: c8:7e:41:f6:25:3b:f5:09:b3:17:e8:46:3d:bf:d0:9b
     * SHA1 Fingerprint: 8f:6b:f2:a9:27:4a:da:14:a0:c4:f4:8e:61:27:f9:c0:1e:78:5d:d1
     * SHA256 Fingerprint: 88:f4:38:dc:f8:ff:d1:fa:8f:42:91:15:ff:e5:f8:2a:e1:e0:6e:0c:70:c3:75:fa:ad:71:7b:34:a4:9e:72:65
     * -----BEGIN CERTIFICATE-----
     * MIIFojCCA4qgAwIBAgIUAZQwHqIL3fXFMyqxQ0Rx+NZQTQ0wDQYJKoZIhvcNAQEM
     * BQAwaTELMAkGA1UEBhMCS1IxJjAkBgNVBAoMHU5BVkVSIEJVU0lORVNTIFBMQVRG
     * T1JNIENvcnAuMTIwMAYDVQQDDClOQVZFUiBHbG9iYWwgUm9vdCBDZXJ0aWZpY2F0
     * aW9uIEF1dGhvcml0eTAeFw0xNzA4MTgwODU4NDJaFw0zNzA4MTgyMzU5NTlaMGkx
     * CzAJBgNVBAYTAktSMSYwJAYDVQQKDB1OQVZFUiBCVVNJTkVTUyBQTEFURk9STSBD
     * b3JwLjEyMDAGA1UEAwwpTkFWRVIgR2xvYmFsIFJvb3QgQ2VydGlmaWNhdGlvbiBB
     * dXRob3JpdHkwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQC21PGTXLVA
     * iQqrDZBbUGOukJR0F0Vy1ntlWilLp1agS7gvQnXp2XskWjFlqxcX0TM62RHcQDaH
     * 38dq6SZeWYp34+hInDEW+j6RscrJo+KfziFTowI2MMtSAuXaMl3Dxeb57hHHi8lE
     * HoSTGEq0n+USZGnQJoViAbbJAh2+g1G7XNr4rRVqmfeSVPc0W+m/6imBEtRTkZaz
     * kVrd/pBzKPswRrXKCAfHcXLJZtM0l/aM9BhK4dA9WkW2aacp+yPOiNgSnABIqKYP
     * szuSjXEOdMWLyEz59JuOuDxp7W87UC9Y7cSw0BwbagzivESq2M0UXZR4Yb8Obtoq
     * vC8MC3GmsxY/nOb5zJ9TNeIDoKAYv7vxvvTWjIcNQvcGufFt7QSUqP620wbGQGHf
     * nZ3zVHbOUzoBppJB7ASjjw2i1QnK1sua8e9DXcCrpUHPXFNwcMmIpi3Ua2FzUCaG
     * YQ5fG8Ir4ozVu53BA0K6lNpfqbDKzE0K70dpAy8i+/Eozr9dUGWokG2zdLAIx6yo
     * 0es+nPxdGoMuK8u180SdOqcXYZaicdNwlhVNt0xz7hlcxVs+Qf6sdWA7G2POAN3a
     * CJBitOUt7kinaxeZVL6HSuOpXgRM6xBtVNbv8ejyYhbLgGvtPe31HzClrkvJE+2K
     * AQHJuFFYwGY6sWZLxNUxAmLpdIQM201GLQIDAQABo0IwQDAdBgNVHQ4EFgQU0p+I
     * 36HNLL3s9TsBAZMzJ7LrYEswDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMB
     * Af8wDQYJKoZIhvcNAQEMBQADggIBADLKgLOdPVQG3dLSLvCkASELZ0jKbY7gyKoN
     * qo0hV4/GPnrK21HUUrPUloSlWGB/5QuOH/XcChWB5Tu2tyIvCZwTFrFsDDUIbatj
     * cu3cvuzHV+YwIHHW1xDBE1UBjCpD5EHxzzp6U5LOogMFDTjfArsQLtk70pt6wKGm
     * +LUx5vR1yblTmXVHIloUFcd4G7ad6Qz4G3bxhYTeodoS76TiEJd6eN4MUZeoIUCL
     * hr0N8F5OSza7OyAfikJW4Qsav3vQIkMsRIz75Sq0bBwcupTgE34h5prCy8VCZLQe
     * lHsIJchxzIdFV4XTnyliIoNRlwAYl3dqmJLJfGBs32x9SuRwTMKeuB330DTHD8z7
     * p/8Dvq1wkNoL3chtl1+afwkyQf3NosxabUzyqkn+Zvjp2DXrDige7kgvOtB5CTh8
     * piKCk5XQA76+AqAF3SAi428diDRgxuYKuQl1C/AH6GmWNcf7I4GOODm4RStDeKLR
     * LBT/DShycpWbXgnbiUSYqqFJu3FS8r/2/yehNq+4tneI3TqkbZs0kNwUXTC/t+sX
     * 5Ie3cdCh13cV1ELX8vMxmV2b3RZtP+oGI/hGoiLtk/bdmuYqh7GYVPEi92tF4+KO
     * dh2ajcQGjTa3FPOdVGm3jjzVpG2Tgbet9r1ke8LJaDmgkpzNNIaRkPpkUZ3+/uul
     * 9XXeifdy
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02KR1&0$\x06\x03U\x04\n\x0c\x1dNAVER BUSINESS PLATFORM Corp.1200\x06\x03U\x04\x03\x0c)NAVER Global Root Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb6\xd4\xf1\x93\\\xb5@\x89\n\xab\r\x90[Pc\xae\x90\x94t\x17Er\xd6{eZ)K\xa7V\xa0K\xb8/Bu\xe9\xd9{$Z1e\xab\x17\x17\xd13:\xd9\x11\xdc@6\x87\xdf\xc7j\xe9&^Y\x8aw\xe3\xe8H\x9c1\x16\xfa>\x91\xb1\xca\xc9\xa3\xe2\x9f\xce!S\xa3\x0260\xcbR\x02\xe5\xda2]\xc3\xc5\xe6\xf9\xee\x11\xc7\x8b\xc9D\x1e\x84\x93\x18J\xb4\x9f\xe5\x12di\xd0&\x85b\x01\xb6\xc9\x02\x1d\xbe\x83Q\xbb\\\xda\xf8\xad\x15j\x99\xf7\x92T\xf74[\xe9\xbf\xea)\x81\x12\xd4S\x91\x96\xb3\x91Z\xdd\xfe\x90s(\xfb0F\xb5\xca\x08\x07\xc7qr\xc9f\xd34\x97\xf6\x8c\xf4\x18J\xe1\xd0=ZE\xb6i\xa7)\xfb#\xce\x88\xd8\x12\x9c\x00H\xa8\xa6\x0f\xb3;\x92\x8dq\x0et\xc5\x8b\xc8L\xf9\xf4\x9b\x8e\xb8<i\xedo;P/X\xed\xc4\xb0\xd0\x1c\x1bj\x0c\xe2\xbcD\xaa\xd8\xcd\x14]\x94xa\xbf\x0en\xda*\xbc/\x0c\x0bq\xa6\xb3\x16?\x9c\xe6\xf9\xcc\x9fS5\xe2\x03\xa0\xa0\x18\xbf\xbb\xf1\xbe\xf4\xd6\x8c\x87\rB\xf7\x06\xb9\xf1m\xed\x04\x94\xa8\xfe\xb6\xd3\x06\xc6@a\xdf\x9d\x9d\xf3Tv\xceS:\x01\xa6\x92A\xec\x04\xa3\x8f\r\xa2\xd5\t\xca\xd6\xcb\x9a\xf1\xefC]\xc0\xab\xa5A\xcf\\Spp\xc9\x88\xa6-\xd4kasP&\x86a\x0e_\x1b\xc2+\xe2\x8c\xd5\xbb\x9d\xc1\x03B\xba\x94\xda_\xa9\xb0\xca\xccM\n\xefGi\x03/\"\xfb\xf1(\xce\xbf]Pe\xa8\x90m\xb3t\xb0\x08\xc7\xac\xa8\xd1\xeb>\x9c\xfc]\x1a\x83.+\xcb\xb5\xf3D\x9d:\xa7\x17a\x96\xa2q\xd3p\x96\x15M\xb7Ls\xee\x19\\\xc5[>A\xfe\xacu`;\x1bc\xce\x00\xdd\xda\x08\x90b\xb4\xe5-\xeeH\xa7k\x17\x99T\xbe\x87J\xe3\xa9^\x04L\xeb\x10mT\xd6\xef\xf1\xe8\xf2b\x16\xcb\x80k\xed=\xed\xf5\x1f0\xa5\xaeK\xc9\x13\xed\x8a\x01\x01\xc9\xb8QX\xc0f:\xb1fK\xc4\xd51\x02b\xe9t\x84\x0c\xdbMF-\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root CA 1 G3 O=QuoVadis Limited
     * Subject: CN=QuoVadis Root CA 1 G3 O=QuoVadis Limited
     * Label: "QuoVadis Root CA 1 G3"
     * Serial: 687049649626669250736271037606554624078720034195
     * MD5 Fingerprint: a4:bc:5b:3f:fe:37:9a:fa:64:f0:e2:fa:05:3d:0b:ab
     * SHA1 Fingerprint: 1b:8e:ea:57:96:29:1a:c9:39:ea:b8:0a:81:1a:73:73:c0:93:79:67
     * SHA256 Fingerprint: 8a:86:6f:d1:b2:76:b5:7e:57:8e:92:1c:65:82:8a:2b:ed:58:e9:f2:f2:88:05:41:34:b7:f1:f4:bf:c9:cc:74
     * -----BEGIN CERTIFICATE-----
     * MIIFYDCCA0igAwIBAgIUeFhfLq0sGUvjNwc1NBMotZbUZZMwDQYJKoZIhvcNAQEL
     * BQAwSDELMAkGA1UEBhMCQk0xGTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxHjAc
     * BgNVBAMTFVF1b1ZhZGlzIFJvb3QgQ0EgMSBHMzAeFw0xMjAxMTIxNzI3NDRaFw00
     * MjAxMTIxNzI3NDRaMEgxCzAJBgNVBAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBM
     * aW1pdGVkMR4wHAYDVQQDExVRdW9WYWRpcyBSb290IENBIDEgRzMwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQCgvlAQjunybEC0BJyFuTHK3C3kEakEPBtV
     * wedYMB0ktMPvhd6MLOHBPd+C5k+tR4ds7FtJwUrVu4/sh6x/gpqG7D0DmVIB0jWe
     * rNrwU8lmPNSsAgHaJNM7qAJGr6Qc4/hzWHa39g6QDbXwz8z6+cZM5cOGMAqNF341
     * 68Xfuw6cwI2H44g4hWf6Pser4BOcBRiYz5P1sZK0/CPTz9XEJ0ngnjybCKOLXSoh
     * 4Pw5qlPafX7PGglTvF0FBM+hSo+LdoINofjSxxR3W5A2B4GbPgb6Ul5jxaYA/qXp
     * UhtStZI5cgMJYr2wYBZupt0lwgNm3fME0UDiTouG9G/lg6AnhF4EwfWQvTA9xO+o
     * abw4m6SkltFi2mnAAZauy8RRNOoMqv8hjlmPSlzkYZqn0ukqeI1RPToV7qJZjqlc
     * 3sX5kCLliEVx3ZGZbHqfPT2YfF72vhZooF6uCyP8Wg+qInYtyaEQHeTTRCOQiJ/G
     * KubX9ZqzWB4vMIkIG1SitZgj7Ah3HJVdYdHLiZxfokqRmu8hqkkWCKi9YSgxyXSt
     * hfbZxbGL0eUQMk1fiyA6PEkfM4VZDdvLCXVDaXP7a3F98N/ETH3Goy7IlXnLc6KO
     * Tk0k+17kBL5yG6YnLUlamXrXXAkgt3+UuU/xDRxeiEIbEbfnkduebPRq34wGmAOt
     * zCjvpUfzUwIDAQABo0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIB
     * BjAdBgNVHQ4EFgQUo5fW816iEOGrRZ88F2Q87gFwnMwwDQYJKoZIhvcNAQELBQAD
     * ggIBABj6W3X8PnrHX3fHyt/PX8MSxEBd1DKquGrX1RUVRpgjpeaQWxiZTOOtQqOC
     * MTaIzen7xASWSIsBx40Bz1szBpZGZnQdT+3Btrm0DWHMY37XLneMlhwqI2hrhVd2
     * cDMT/uFPpiN3GPoajOi9ZcnPP/TJF9zrx7zABC4tRi9pZsMbj/7sPtPKlL92CiUN
     * qXsCHKnQO18LwIE6PWThv6ctTr1NxNgpxiIY0MWscgKCP6o6ojoilzHdCGPDdRS5
     * YCgtW2jgFqlmgiNR9etT2DGbe+m3nUvriBbP+V04ikkwj+3x6xn0dxoxGE1nVGwv
     * b2X52z3sIexe9PSLymBlVNFxZPT5pqOBMzYzcfCkeF9OrYMh3jRJjehZrJ3ydlo2
     * 8hP0r+AJx2EqbPfgna67hkooby7utHnNkDPDs3b69fBsnQGQ+p6Q9pxyz0fawx/k
     * NSBT8lTR32GDpgLiJTjehTItXnOQUl1CxM49S+H5GYQd1aJQzEH7QRTDvdbJWqNj
     * ZgKAvQU6O0ec7AAmTPWIUb+oI38YB7AL7YsmoWTTYUrrXJ/es69nA7Mf3W1daWhp
     * q1467HxpvMc7hU6eFbm0FU/DlXpY18ls6Wy58yljXrQs8C097Vpl4KlbQMJImYFt
     * nh8GKjwStIsPm6Ik8KaN1nrgS7ZklmOVhMJKzRwuJIczYOXD
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1\x1e0\x1c\x06\x03U\x04\x03\x13\x15QuoVadis Root CA 1 G3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa0\xbeP\x10\x8e\xe9\xf2l@\xb4\x04\x9c\x85\xb91\xca\xdc-\xe4\x11\xa9\x04<\x1bU\xc1\xe7X0\x1d$\xb4\xc3\xef\x85\xde\x8c,\xe1\xc1=\xdf\x82\xe6O\xadG\x87l\xec[I\xc1J\xd5\xbb\x8f\xec\x87\xac\x7f\x82\x9a\x86\xec=\x03\x99R\x01\xd25\x9e\xac\xda\xf0S\xc9f<\xd4\xac\x02\x01\xda$\xd3;\xa8\x02F\xaf\xa4\x1c\xe3\xf8sXv\xb7\xf6\x0e\x90\r\xb5\xf0\xcf\xcc\xfa\xf9\xc6L\xe5\xc3\x860\n\x8d\x17~5\xeb\xc5\xdf\xbb\x0e\x9c\xc0\x8d\x87\xe3\x888\x85g\xfa>\xc7\xab\xe0\x13\x9c\x05\x18\x98\xcf\x93\xf5\xb1\x92\xb4\xfc#\xd3\xcf\xd5\xc4\'I\xe0\x9e<\x9b\x08\xa3\x8b]*!\xe0\xfc9\xaaS\xda}~\xcf\x1a\tS\xbc]\x05\x04\xcf\xa1J\x8f\x8bv\x82\r\xa1\xf8\xd2\xc7\x14w[\x906\x07\x81\x9b>\x06\xfaR^c\xc5\xa6\x00\xfe\xa5\xe9R\x1bR\xb5\x929r\x03\tb\xbd\xb0`\x16n\xa6\xdd%\xc2\x03f\xdd\xf3\x04\xd1@\xe2N\x8b\x86\xf4o\xe5\x83\xa0\'\x84^\x04\xc1\xf5\x90\xbd0=\xc4\xef\xa8i\xbc8\x9b\xa4\xa4\x96\xd1b\xdai\xc0\x01\x96\xae\xcb\xc4Q4\xea\x0c\xaa\xff!\x8eY\x8fJ\\\xe4a\x9a\xa7\xd2\xe9*x\x8dQ=:\x15\xee\xa2Y\x8e\xa9\\\xde\xc5\xf9\x90\"\xe5\x88Eq\xdd\x91\x99lz\x9f==\x98|^\xf6\xbe\x16h\xa0^\xae\x0b#\xfcZ\x0f\xaa\"v-\xc9\xa1\x10\x1d\xe4\xd3D#\x90\x88\x9f\xc6*\xe6\xd7\xf5\x9a\xb3X\x1e/0\x89\x08\x1bT\xa2\xb5\x98#\xec\x08w\x1c\x95]a\xd1\xcb\x89\x9c_\xa2J\x91\x9a\xef!\xaaI\x16\x08\xa8\xbda(1\xc9t\xad\x85\xf6\xd9\xc5\xb1\x8b\xd1\xe5\x102M_\x8b :<I\x1f3\x85Y\r\xdb\xcb\tuCis\xfbkq}\xf0\xdf\xc4L}\xc6\xa3.\xc8\x95y\xcbs\xa2\x8eNM$\xfb^\xe4\x04\xber\x1b\xa6\'-IZ\x99z\xd7\\\t \xb7\x7f\x94\xb9O\xf1\r\x1c^\x88B\x1b\x11\xb7\xe7\x91\xdb\x9el\xf4j\xdf\x8c\x06\x98\x03\xad\xcc(\xef\xa5G\xf3S\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Amazon Root CA 1 O=Amazon
     * Subject: CN=Amazon Root CA 1 O=Amazon
     * Label: "Amazon Root CA 1"
     * Serial: 143266978916655856878034712317230054538369994
     * MD5 Fingerprint: 43:c6:bf:ae:ec:fe:ad:2f:18:c6:88:68:30:fc:c8:e6
     * SHA1 Fingerprint: 8d:a7:f9:65:ec:5e:fc:37:91:0f:1c:6e:59:fd:c1:cc:6a:6e:de:16
     * SHA256 Fingerprint: 8e:cd:e6:88:4f:3d:87:b1:12:5b:a3:1a:c3:fc:b1:3d:70:16:de:7f:57:cc:90:4f:e1:cb:97:c6:ae:98:19:6e
     * -----BEGIN CERTIFICATE-----
     * MIIDQTCCAimgAwIBAgITBmyfz5m/jAo54vB4ikPmljZbyjANBgkqhkiG9w0BAQsF
     * ADA5MQswCQYDVQQGEwJVUzEPMA0GA1UEChMGQW1hem9uMRkwFwYDVQQDExBBbWF6
     * b24gUm9vdCBDQSAxMB4XDTE1MDUyNjAwMDAwMFoXDTM4MDExNzAwMDAwMFowOTEL
     * MAkGA1UEBhMCVVMxDzANBgNVBAoTBkFtYXpvbjEZMBcGA1UEAxMQQW1hem9uIFJv
     * b3QgQ0EgMTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBALJ4gHHKeNXj
     * ca9HgFB0fW7Y14h29Jlo91ghYPl0hAEvrAIthtOgQ3pOsqTQNroBvo3bSMgHFzZM
     * 9O6II8c+6zf1tRn4SWiw3te5djgdYZ6k/oI2peVKVuRF4fn9tBb6dNqcmzU5L/qw
     * IFAGbHrQgLKm+a/sRxmPUDgH3KKHOVj4utWp+UhnMJbulHheb4mjUcAwhmahRWa6
     * VOujw5H5SNz/0egwLX0tdHA114gk957EWW67c4cX8jJGKLhD+rcdqsq08p8kDi1L
     * 93FcXmn/6pUCyziKrlA4b9v7LWIbxcceVOF34GfID5yHI9Y/QCB/IIDEgEw+OyQm
     * jgSubJrIqg0CAwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMC
     * AYYwHQYDVR0OBBYEFIQYzIU07LwMlJQuCFmcx7IQTgoIMA0GCSqGSIb3DQEBCwUA
     * A4IBAQCY8jdaQZChGsV2USggNiMOruYou6r4lK5IpDB/G/wkjUu0yKGX9rbxenDI
     * U5PMCCjjmCXPI6T53iHTfIUJrU6adTrCC2qJeHZERxhlbI1Bjjt/msv0tadQ1wUs
     * N+gDS63pYaACbvXy8MWy7Vu33PqUXHeeE6V/Uq2V8viTO96LXFvKWlJbYK8U90vv
     * o/ufQJVtMVT8QtPHRh8jrdkPSHCa2XV4cdFyQzR1bldZwgJcJmApzyMZFo6IQ6XU
     * 5MsI+yMRQ+hDKXJioaldXgjUkK642M4UwtBV8ob2xJNDd2ZhwLnoQdeXeGADbkpy
     * rqXRfboQnoZsG4q5WTP468SQvvG5
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0f0\r\x06\x03U\x04\n\x13\x06Amazon1\x190\x17\x06\x03U\x04\x03\x13\x10Amazon Root CA 1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb2x\x80q\xcax\xd5\xe3q\xafG\x80Pt}n\xd8\xd7\x88v\xf4\x99h\xf7X!`\xf9t\x84\x01/\xac\x02-\x86\xd3\xa0CzN\xb2\xa4\xd06\xba\x01\xbe\x8d\xdbH\xc8\x07\x176L\xf4\xee\x88#\xc7>\xeb7\xf5\xb5\x19\xf8Ih\xb0\xde\xd7\xb9v8\x1da\x9e\xa4\xfe\x826\xa5\xe5JV\xe4E\xe1\xf9\xfd\xb4\x16\xfat\xda\x9c\x9b59/\xfa\xb0 P\x06lz\xd0\x80\xb2\xa6\xf9\xaf\xecG\x19\x8fP8\x07\xdc\xa2\x879X\xf8\xba\xd5\xa9\xf9Hg0\x96\xee\x94x^o\x89\xa3Q\xc00\x86f\xa1Ef\xbaT\xeb\xa3\xc3\x91\xf9H\xdc\xff\xd1\xe80-}-tp5\xd7\x88$\xf7\x9e\xc4Yn\xbbs\x87\x17\xf22F(\xb8C\xfa\xb7\x1d\xaa\xca\xb4\xf2\x9f$\x0e-K\xf7q\\^i\xff\xea\x95\x02\xcb8\x8a\xaeP8o\xdb\xfb-b\x1b\xc5\xc7\x1eT\xe1w\xe0g\xc8\x0f\x9c\x87#\xd6?@ \x7f \x80\xc4\x80L>;$&\x8e\x04\xael\x9a\xc8\xaa\r\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root CA 2 G3 O=QuoVadis Limited
     * Subject: CN=QuoVadis Root CA 2 G3 O=QuoVadis Limited
     * Label: "QuoVadis Root CA 2 G3"
     * Serial: 390156079458959257446133169266079962026824725800
     * MD5 Fingerprint: af:0c:86:6e:bf:40:2d:7f:0b:3e:12:50:ba:12:3d:06
     * SHA1 Fingerprint: 09:3c:61:f3:8b:8b:dc:7d:55:df:75:38:02:05:00:e1:25:f5:c8:36
     * SHA256 Fingerprint: 8f:e4:fb:0a:f9:3a:4d:0d:67:db:0b:eb:b2:3e:37:c7:1b:f3:25:dc:bc:dd:24:0e:a0:4d:af:58:b4:7e:18:40
     * -----BEGIN CERTIFICATE-----
     * MIIFYDCCA0igAwIBAgIURFc0JFuBiZs18s64KztbpybwdSgwDQYJKoZIhvcNAQEL
     * BQAwSDELMAkGA1UEBhMCQk0xGTAXBgNVBAoTEFF1b1ZhZGlzIExpbWl0ZWQxHjAc
     * BgNVBAMTFVF1b1ZhZGlzIFJvb3QgQ0EgMiBHMzAeFw0xMjAxMTIxODU5MzJaFw00
     * MjAxMTIxODU5MzJaMEgxCzAJBgNVBAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBM
     * aW1pdGVkMR4wHAYDVQQDExVRdW9WYWRpcyBSb290IENBIDIgRzMwggIiMA0GCSqG
     * SIb3DQEBAQUAA4ICDwAwggIKAoICAQChriWyARjcV4g/Ruv5r+LrI3HimtFhZiFf
     * qq8nUeVuGxbULX1QsFN3vXg6YOJkApt8hpvWGo6t/x8Vf9WVHhLL5hSEBMHfNrMW
     * n4rjyduYNM7YMxcoRvynyfDStNVNCXJJ+fKH46nafaF9a7I6JaltUkSs+L5u+9ym
     * c5GQYaYDFCDy54ejiK2toIz/pgslUiXnFgHVy7g1gQyjO/Dh4fxaXc6AcW34Sas+
     * O7q414AB+6XrW7PFXmAqMaCvN+ggOp+oMiwMzAkd056OXbxMmO7FGmh77FOm6RQ1
     * o9/NgJ8MSPsc9PG/Srj61YxxSscfrf5BmrODXfKEVu+lV0POKa2Mq1W/xPtbAd0j
     * IaFYAI7D0GoT7RPjEiuA3GfmlbLNHiJuKvhB1PLKFAeNilUSxmn1uIZoL1NesNKq
     * IcGY5jDjZ1XHm26sGahVpkUG0CM62+tlXSoREfA7T8pt9DTEceT/AFr2XK4jYIVz
     * 8eQQsSWu1ZK7E8EM4DnatDlXtas1qnIhO4M15zHfeiFuuDIIfR0ykRVKYnLP43eh
     * vNURG3YBZwjgQQvD6xVu+KQZ2aKrr+InUlYrAoosFCT5v0ICvybIxo/gbjh9Uy3l
     * 7ZizlWNof/k19N+IxWA1ksB8aRxhlRbQ694Lrz4EEEVlWFA4r0jyWbYW8jwNkALG
     * cC4BrTwV1wIDAQABo0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIB
     * BjAdBgNVHQ4EFgQU7edvdlq/YOxJW8ald7tyFnGbxD0wDQYJKoZIhvcNAQELBQAD
     * ggIBAJHfgD9DCX5xwvfrs4iP4VGyvD11+ShdyLyZm3tdquXK4Qr36LLTn91nMX66
     * AarHakE7kNQIXLJgapDwyM4DYvmL7ftuKtwGTTwpD4kWilhMSA/ohGHqPHKmd+RC
     * roijQ1h5fq7KpVMNqT1wvSAZYaRsOPxDMuHBR//47PERIjKWnML2W2mWeyAMQ0Ga
     * W/ZZGYjeVYg3UQt4XAoeo0L9x52ID8DyeAIkVJOviYeIyUqAHerQbj5hLja7NQ4n
     * lv1mNDthcnPxFlxHBlRJAHpYErAK74X9sbgzdWqTHBLmYF5vHX/JHyPLhGGfHoJE
     * +V+tYlUkmlKY7VHnoX6XOuYvHxHaU4AshZ6rNRDbIl9qxV6XU/IyAgkwo1jwDQHV
     * csaxfGl7w/U2Rcxhbl5MlMVerugOXou/983g7aEOGzPuVBj+D77vfoRrQ+NwmNtd
     * dbINWQeFFSM51vHfqSYP1kjHs6Yi9TM3WpVHn3u6GBVv/9YUZINJ0gpnIdsPNWNg
     * KCLjsZWDzYWm3S8P52dSbrsvhXz1SnPnxT7AvSESBT/8twNJAlvIJebiVDj1eYeM
     * HVOyToV7BjjHLPj4sHKNJeV3UvQDHEimUF+IIDBu8oJDqz2XhOdT+yHBTw8imoa4
     * WSr2Rz0ZiC3oheGe7IUIarFsNMkd7EgrO3jtZsSOeWmD3n+M
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1\x1e0\x1c\x06\x03U\x04\x03\x13\x15QuoVadis Root CA 2 G3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa1\xae%\xb2\x01\x18\xdcW\x88?F\xeb\xf9\xaf\xe2\xeb#q\xe2\x9a\xd1af!_\xaa\xaf\'Q\xe5n\x1b\x16\xd4-}P\xb0Sw\xbdx:`\xe2d\x02\x9b|\x86\x9b\xd6\x1a\x8e\xad\xff\x1f\x15\x7f\xd5\x95\x1e\x12\xcb\xe6\x14\x84\x04\xc1\xdf6\xb3\x16\x9f\x8a\xe3\xc9\xdb\x984\xce\xd83\x17(F\xfc\xa7\xc9\xf0\xd2\xb4\xd5M\trI\xf9\xf2\x87\xe3\xa9\xda}\xa1}k\xb2:%\xa9mRD\xac\xf8\xben\xfb\xdc\xa6s\x91\x90a\xa6\x03\x14 \xf2\xe7\x87\xa3\x88\xad\xad\xa0\x8c\xff\xa6\x0b%R%\xe7\x16\x01\xd5\xcb\xb85\x81\x0c\xa3;\xf0\xe1\xe1\xfcZ]\xce\x80qm\xf8I\xab>;\xba\xb8\xd7\x80\x01\xfb\xa5\xeb[\xb3\xc5^`*1\xa0\xaf7\xe8 :\x9f\xa82,\x0c\xcc\t\x1d\xd3\x9e\x8e]\xbcL\x98\xee\xc5\x1ah{\xecS\xa6\xe9\x145\xa3\xdf\xcd\x80\x9f\x0cH\xfb\x1c\xf4\xf1\xbfJ\xb8\xfa\xd5\x8cqJ\xc7\x1f\xad\xfeA\x9a\xb3\x83]\xf2\x84V\xef\xa5WC\xce)\xad\x8c\xabU\xbf\xc4\xfb[\x01\xdd#!\xa1X\x00\x8e\xc3\xd0j\x13\xed\x13\xe3\x12+\x80\xdcg\xe6\x95\xb2\xcd\x1e\"n*\xf8A\xd4\xf2\xca\x14\x07\x8d\x8aU\x12\xc6i\xf5\xb8\x86h/S^\xb0\xd2\xaa!\xc1\x98\xe60\xe3gU\xc7\x9bn\xac\x19\xa8U\xa6E\x06\xd0#:\xdb\xebe]*\x11\x11\xf0;O\xcam\xf44\xc4q\xe4\xff\x00Z\xf6\\\xae#`\x85s\xf1\xe4\x10\xb1%\xae\xd5\x92\xbb\x13\xc1\x0c\xe09\xda\xb49W\xb5\xab5\xaar!;\x835\xe71\xdfz!n\xb82\x08}\x1d2\x91\x15Jbr\xcf\xe3w\xa1\xbc\xd5\x11\x1bv\x01g\x08\xe0A\x0b\xc3\xeb\x15n\xf8\xa4\x19\xd9\xa2\xab\xaf\xe2\'RV+\x02\x8a,\x14$\xf9\xbfB\x02\xbf&\xc8\xc6\x8f\xe0n8}S-\xe5\xed\x98\xb3\x95ch\x7f\xf95\xf4\xdf\x88\xc5`5\x92\xc0|i\x1ca\x95\x16\xd0\xeb\xde\x0b\xaf>\x04\x10EeXP8\xafH\xf2Y\xb6\x16\xf2<\r\x90\x02\xc6p.\x01\xad<\x15\xd7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=T-TeleSec GlobalRoot Class 2 O=T-Systems Enterprise Services GmbH OU=T-Systems Trust Center
     * Subject: CN=T-TeleSec GlobalRoot Class 2 O=T-Systems Enterprise Services GmbH OU=T-Systems Trust Center
     * Label: "T-TeleSec GlobalRoot Class 2"
     * Serial: 1
     * MD5 Fingerprint: 2b:9b:9e:e4:7b:6c:1f:00:72:1a:cc:c1:77:79:df:6a
     * SHA1 Fingerprint: 59:0d:2d:7d:88:4f:40:2e:61:7e:a5:62:32:17:65:cf:17:d8:94:e9
     * SHA256 Fingerprint: 91:e2:f5:78:8d:58:10:eb:a7:ba:58:73:7d:e1:54:8a:8e:ca:cd:01:45:98:bc:0b:14:3e:04:1b:17:05:25:52
     * -----BEGIN CERTIFICATE-----
     * MIIDwzCCAqugAwIBAgIBATANBgkqhkiG9w0BAQsFADCBgjELMAkGA1UEBhMCREUx
     * KzApBgNVBAoMIlQtU3lzdGVtcyBFbnRlcnByaXNlIFNlcnZpY2VzIEdtYkgxHzAd
     * BgNVBAsMFlQtU3lzdGVtcyBUcnVzdCBDZW50ZXIxJTAjBgNVBAMMHFQtVGVsZVNl
     * YyBHbG9iYWxSb290IENsYXNzIDIwHhcNMDgxMDAxMTA0MDE0WhcNMzMxMDAxMjM1
     * OTU5WjCBgjELMAkGA1UEBhMCREUxKzApBgNVBAoMIlQtU3lzdGVtcyBFbnRlcnBy
     * aXNlIFNlcnZpY2VzIEdtYkgxHzAdBgNVBAsMFlQtU3lzdGVtcyBUcnVzdCBDZW50
     * ZXIxJTAjBgNVBAMMHFQtVGVsZVNlYyBHbG9iYWxSb290IENsYXNzIDIwggEiMA0G
     * CSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCqX9obX+hzkeXaXPSi5kfl82hVYAUd
     * AqSzm1nzHoqvNK38DcLZSBnuaY/JIPwhqgcZ7bBcrGXHX+0CfHt8LRvWurmAwhiC
     * FoT6ZrAIxlQjgeTNuUk/9k9uN0goOA/FvudocP05l03Sx5iRUKrERLMjfTlH6VJi
     * 1hKTXrcxlkIF+3anHqP1wvzpesVsqXFP6st4vGCvx9702cu+fjOlbpSD8DT6Iavq
     * jnKgP6TeMFvvhk1qlVtDRKgQFRzlAVfFmPHmBiiRqiDFt1MmUUOyCxGVWOHAD3bZ
     * wI18gfNycJ5v/hqO2V81xrJvNHy+SE/iWjnX2J14np+GPgNeGYtEotXHAgMBAAGj
     * QjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMB0GA1UdDgQWBBS/
     * WSA2AHmgoCJrjNXyYdK4LMuCSjANBgkqhkiG9w0BAQsFAAOCAQEAMQOiYQsfdOhy
     * NsZt+U2e+iKo4YFWz827n+qrkRk4r6p8FU3ztqONpfSO9kSpp+ghla0+AGIWiPAC
     * uvxhI+YzmzB6azZie60EI4RYZeLbK4rnJVM3YlNfvNoBYimipidx5joifsFvHZVw
     * IEoHNN/q/xWA5brXethbdXwFeilHfkCoMRN3zUA7tFFHei4R40cR3p1m0IvVVGb6
     * g1XqfMIpiRvpb7PO4gWEyS8+eIVibslfwXhjdFjASBgMmTnrpMwatXlajRWc2BQN
     * 9noHV8cigwUtPJslJj0Ys6lDfMjIq2SPDqO/nBudMNva0Bkuqjzx+zOAduTNrRlP
     * BSeOE6Fuwg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02DE1+0)\x06\x03U\x04\n\x0c\"T-Systems Enterprise Services GmbH1\x1f0\x1d\x06\x03U\x04\x0b\x0c\x16T-Systems Trust Center1%0#\x06\x03U\x04\x03\x0c\x1cT-TeleSec GlobalRoot Class 2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xaa_\xda\x1b_\xe8s\x91\xe5\xda\\\xf4\xa2\xe6G\xe5\xf3hU`\x05\x1d\x02\xa4\xb3\x9bY\xf3\x1e\x8a\xaf4\xad\xfc\r\xc2\xd9H\x19\xeei\x8f\xc9 \xfc!\xaa\x07\x19\xed\xb0\\\xace\xc7_\xed\x02|{|-\x1b\xd6\xba\xb9\x80\xc2\x18\x82\x16\x84\xfaf\xb0\x08\xc6T#\x81\xe4\xcd\xb9I?\xf6On7H(8\x0f\xc5\xbe\xe7hp\xfd9\x97M\xd2\xc7\x98\x91P\xaa\xc4D\xb3#}9G\xe9Rb\xd6\x12\x93^\xb71\x96B\x05\xfbv\xa7\x1e\xa3\xf5\xc2\xfc\xe9z\xc5l\xa9qO\xea\xcbx\xbc`\xaf\xc7\xde\xf4\xd9\xcb\xbe~3\xa5n\x94\x83\xf04\xfa!\xab\xea\x8er\xa0?\xa4\xde0[\xef\x86Mj\x95[CD\xa8\x10\x15\x1c\xe5\x01W\xc5\x98\xf1\xe6\x06(\x91\xaa \xc5\xb7S&QC\xb2\x0b\x11\x95X\xe1\xc0\x0fv\xd9\xc0\x8d|\x81\xf3rp\x9eo\xfe\x1a\x8e\xd9_5\xc6\xb2o4|\xbeHO\xe2Z9\xd7\xd8\x9dx\x9e\x9f\x86>\x03^\x19\x8bD\xa2\xd5\xc7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Trustwave Global ECC P256 Certification Authority O=Trustwave Holdings, Inc.
     * Subject: CN=Trustwave Global ECC P256 Certification Authority O=Trustwave Holdings, Inc.
     * Label: "Trustwave Global ECC P256 Certification Authority"
     * Serial: 4151900041497450638097112925
     * MD5 Fingerprint: 5b:44:e3:8d:5d:36:86:26:e8:0d:05:d2:59:a7:83:54
     * SHA1 Fingerprint: b4:90:82:dd:45:0c:be:8b:5b:b1:66:d3:e2:a4:08:26:cd:ed:42:cf
     * SHA256 Fingerprint: 94:5b:bc:82:5e:a5:54:f4:89:d1:fd:51:a7:3d:df:2e:a6:24:ac:70:19:a0:52:05:22:5c:22:a7:8c:cf:a8:b4
     * -----BEGIN CERTIFICATE-----
     * MIICYDCCAgegAwIBAgIMDWpfCD8oXD5Rld9dMAoGCCqGSM49BAMCMIGRMQswCQYD
     * VQQGEwJVUzERMA8GA1UECBMISWxsaW5vaXMxEDAOBgNVBAcTB0NoaWNhZ28xITAf
     * BgNVBAoTGFRydXN0d2F2ZSBIb2xkaW5ncywgSW5jLjE6MDgGA1UEAxMxVHJ1c3R3
     * YXZlIEdsb2JhbCBFQ0MgUDI1NiBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTAeFw0x
     * NzA4MjMxOTM1MTBaFw00MjA4MjMxOTM1MTBaMIGRMQswCQYDVQQGEwJVUzERMA8G
     * A1UECBMISWxsaW5vaXMxEDAOBgNVBAcTB0NoaWNhZ28xITAfBgNVBAoTGFRydXN0
     * d2F2ZSBIb2xkaW5ncywgSW5jLjE6MDgGA1UEAxMxVHJ1c3R3YXZlIEdsb2JhbCBF
     * Q0MgUDI1NiBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTBZMBMGByqGSM49AgEGCCqG
     * SM49AwEHA0IABH77bOYj43MyCMpg5lOcunSNGLB4kFKA3TjASh3RqMyTpJcGOMoN
     * FWLGjgEqZZ2q3zSRLoHB5DOSMcT9CTqmP62jQzBBMA8GA1UdEwEB/wQFMAMBAf8w
     * DwYDVR0PAQH/BAUDAwcGADAdBgNVHQ4EFgQUo0EGrJBt0UrrdaVKEJmzsaGLSvcw
     * CgYIKoZIzj0EAwIDRwAwRAIgB+ZU2g6gWrKuEZ+Hxbb/ad4lvvigtwjzRM4q3wgh
     * DDcCIC0mA6AFvWvR9lz4ZcyGbbOcNEhjhAnFjXca4syc4XR7
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x110\x0f\x06\x03U\x04\x08\x13\x08Illinois1\x100\x0e\x06\x03U\x04\x07\x13\x07Chicago1!0\x1f\x06\x03U\x04\n\x13\x18Trustwave Holdings, Inc.1:08\x06\x03U\x04\x03\x131Trustwave Global ECC P256 Certification Authority",
        spki: b"0\x13\x06\x07*\x86H\xce=\x02\x01\x06\x08*\x86H\xce=\x03\x01\x07\x03B\x00\x04~\xfbl\xe6#\xe3s2\x08\xca`\xe6S\x9c\xbat\x8d\x18\xb0x\x90R\x80\xdd8\xc0J\x1d\xd1\xa8\xcc\x93\xa4\x97\x068\xca\r\x15b\xc6\x8e\x01*e\x9d\xaa\xdf4\x91.\x81\xc1\xe43\x921\xc4\xfd\t:\xa6?\xad",
        name_constraints: None
    },

    /*
     * Issuer: CN=Cybertrust Global Root O=Cybertrust, Inc
     * Subject: CN=Cybertrust Global Root O=Cybertrust, Inc
     * Label: "Cybertrust Global Root"
     * Serial: 4835703278459682877484360
     * MD5 Fingerprint: 72:e4:4a:87:e3:69:40:80:77:ea:bc:e3:f4:ff:f0:e1
     * SHA1 Fingerprint: 5f:43:e5:b1:bf:f8:78:8c:ac:1c:c7:ca:4a:9a:c6:22:2b:cc:34:c6
     * SHA256 Fingerprint: 96:0a:df:00:63:e9:63:56:75:0c:29:65:dd:0a:08:67:da:0b:9c:bd:6e:77:71:4a:ea:fb:23:49:ab:39:3d:a3
     * -----BEGIN CERTIFICATE-----
     * MIIDoTCCAomgAwIBAgILBAAAAAABD4WqLUgwDQYJKoZIhvcNAQEFBQAwOzEYMBYG
     * A1UEChMPQ3liZXJ0cnVzdCwgSW5jMR8wHQYDVQQDExZDeWJlcnRydXN0IEdsb2Jh
     * bCBSb290MB4XDTA2MTIxNTA4MDAwMFoXDTIxMTIxNTA4MDAwMFowOzEYMBYGA1UE
     * ChMPQ3liZXJ0cnVzdCwgSW5jMR8wHQYDVQQDExZDeWJlcnRydXN0IEdsb2JhbCBS
     * b290MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA+Mi8vRRQZhP/8NN5
     * 7CPytxrHjoXxEnOmGaoQ25yiZXRadz5RfVb23CO21O1fWLE3TdVJDm71aofW0ozS
     * J8bi/zafmGWgE07GKmSb1ZASzxQG9Dvj1Ci+6A74q05IlG2OlTEQXO2iLb3VOm2y
     * HLtgwEZLAfVJrn5GitB0jaEMAs7u/OePuGtm839EAL9mJRQr3RAwHQeWP032a7iP
     * t3sMpTjr3kfb1V05/Iin89cqdPHoWqI7n1C6poxFNcJQZZXcY4Lv3b93TZxiyWNz
     * FtApD0mpSPCzqrdsxacwOUBdrsTiXSZT8M4cIwhhqJQZugRiQOwfOHB3EgZxpzAY
     * XSUnpQIDAQABo4GlMIGiMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8EBTADAQH/
     * MB0GA1UdDgQWBBS2CHsNesysIEyGVjJez6tuhS1wVzA/BgNVHR8EODA2MDSgMqAw
     * hi5odHRwOi8vd3d3Mi5wdWJsaWMtdHJ1c3QuY29tL2NybC9jdC9jdHJvb3QuY3Js
     * MB8GA1UdIwQYMBaAFLYIew16zKwgTIZWMl7Pq26FLXBXMA0GCSqGSIb3DQEBBQUA
     * A4IBAQBW7wojoFROlZfJ+InaRcHUowAl9B8Tq7ejhVhpwjCt2BWKLePJzYFa+HMj
     * Wqd8BfP9IjsO0QbE2zZMcwSO5bAi5MXzLqXZI+O4Tkogp24CJJ8iYGd7ix1yCcUx
     * XOl5n4BHPa2hCwcUPUf/A2kaDAtE52Mlp3+yybh2hO0j9n0Hq0V+09+zv+mKts2o
     * omcrUtW3ZfA5TGOgkXmTUg9U3YO7n9GPp1Nzw8v/MOx8BLjYRB+TX3EJIrduPuoc
     * A06dGiBh+4E37F78CkWr1+cXVdCg6mCbpvbjjFspwgZgFJ0tl0ypkxWdYcQBX0jW
     * WL1WMRJOEcgh4LMRkWXbtKaIOM5V
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x180\x16\x06\x03U\x04\n\x13\x0fCybertrust, Inc1\x1f0\x1d\x06\x03U\x04\x03\x13\x16Cybertrust Global Root",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xf8\xc8\xbc\xbd\x14Pf\x13\xff\xf0\xd3y\xec#\xf2\xb7\x1a\xc7\x8e\x85\xf1\x12s\xa6\x19\xaa\x10\xdb\x9c\xa2etZw>Q}V\xf6\xdc#\xb6\xd4\xed_X\xb17M\xd5I\x0en\xf5j\x87\xd6\xd2\x8c\xd2\'\xc6\xe2\xff6\x9f\x98e\xa0\x13N\xc6*d\x9b\xd5\x90\x12\xcf\x14\x06\xf4;\xe3\xd4(\xbe\xe8\x0e\xf8\xabNH\x94m\x8e\x951\x10\\\xed\xa2-\xbd\xd5:m\xb2\x1c\xbb`\xc0FK\x01\xf5I\xae~F\x8a\xd0t\x8d\xa1\x0c\x02\xce\xee\xfc\xe7\x8f\xb8kf\xf3\x7fD\x00\xbff%\x14+\xdd\x100\x1d\x07\x96?M\xf6k\xb8\x8f\xb7{\x0c\xa58\xeb\xdeG\xdb\xd5]9\xfc\x88\xa7\xf3\xd7*t\xf1\xe8Z\xa2;\x9fP\xba\xa6\x8cE5\xc2Pe\x95\xdcc\x82\xef\xdd\xbfwM\x9cb\xc9cs\x16\xd0)\x0fI\xa9H\xf0\xb3\xaa\xb7l\xc5\xa709@]\xae\xc4\xe2]&S\xf0\xce\x1c#\x08a\xa8\x94\x19\xba\x04b@\xec\x1f8pw\x12\x06q\xa70\x18]%\'\xa5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=ISRG Root X1 O=Internet Security Research Group
     * Subject: CN=ISRG Root X1 O=Internet Security Research Group
     * Label: "ISRG Root X1"
     * Serial: 172886928669790476064670243504169061120
     * MD5 Fingerprint: 0c:d2:f9:e0:da:17:73:e9:ed:86:4d:a5:e3:70:e7:4e
     * SHA1 Fingerprint: ca:bd:2a:79:a1:07:6a:31:f2:1d:25:36:35:cb:03:9d:43:29:a5:e8
     * SHA256 Fingerprint: 96:bc:ec:06:26:49:76:f3:74:60:77:9a:cf:28:c5:a7:cf:e8:a3:c0:aa:e1:1a:8f:fc:ee:05:c0:bd:df:08:c6
     * -----BEGIN CERTIFICATE-----
     * MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
     * TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
     * cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
     * WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
     * ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
     * MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
     * h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
     * 0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
     * A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
     * T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
     * B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
     * B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
     * KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
     * OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
     * jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
     * qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
     * rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
     * HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
     * hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
     * ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
     * 3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
     * NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
     * ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
     * TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
     * jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
     * oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
     * 4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
     * mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
     * emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1)0\'\x06\x03U\x04\n\x13 Internet Security Research Group1\x150\x13\x06\x03U\x04\x03\x13\x0cISRG Root X1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xad\xe8$s\xf4\x147\xf3\x9b\x9e+W(\x1c\x87\xbe\xdc\xb7\xdf8\x90\x8cn<\xe6W\xa0x\xf7u\xc2\xa2\xfe\xf5jn\xf6\x00O(\xdb\xdeh\x86lD\x93\xb6\xb1c\xfd\x14\x12k\xbf\x1f\xd2\xea1\x9b!~\xd13<\xbaH\xf5\xddy\xdf\xb3\xb8\xff\x12\xf1!\x9aK\xc1\x8a\x86qiJffl\x8f~<p\xbf\xad)\"\x06\xf3\xe4\xc0\xe6\x80\xae\xe2K\x8f\xb7\x99~\x94\x03\x9f\xd3G\x97|\x99H#S\xe88\xaeO\no\x83.\xd1IW\x8c\x80t\xb6\xda/\xd08\x8d{\x03p!\x1bu\xf20<\xfa\x8f\xae\xdd\xdac\xab\xeb\x16O\xc2\x8e\x11K~\xcf\x0b\xe8\xff\xb5w.\xf4\xb2{J\xe0L\x12%\x0cp\x8d\x03)\xa0\xe1S$\xec\x13\xd9\xee\x19\xbf\x10\xb3J\x8c?\x89\xa3aQ\xde\xac\x87\x07\x94\xf4cq\xec.\xe2o[\x98\x81\xe1\x89\\4ylv\xef;\x90by\xe6\xdb\xa4\x9a/&\xc5\xd0\x10\xe1\x0e\xde\xd9\x10\x8e\x16\xfb\xb7\xf7\xa8\xf7\xc7\xe5\x02\x07\x98\x8f6\x08\x95\xe7\xe27\x96\r6u\x9e\xfb\x0er\xb1\x1d\x9b\xbc\x03\xf9I\x05\xd8\x81\xdd\x05\xb4*\xd6A\xe9\xac\x01v\x95\n\x0f\xd8\xdf\xd5\xbd\x12\x1f5/(\x17l\xd2\x98\xc1\xa8\tdwnG7\xba\xce\xacY^h\x9d\x7fr\xd6\x89\xc5\x06A)>Y>\xdd&\xf5$\xc9\x11\xa7Z\xa3L@\x1fF\xa1\x99\xb5\xa7:Qn\x86;\x9e}r\xa7\x12\x05xY\xed>Qx\x15\x0b\x03\x8f\x8d\xd0/\x05\xb2>{J\x1cKs\x05\x12\xfc\xc6\xea\xe0P\x13|C\x93t\xb3\xcat\xe7\x8e\x1f\x01\x08\xd00\xd4[q6\xb4\x07\xba\xc100\\H\xb7\x82;\x98\xa6}`\x8a\xa2\xa3)\x82\xcc\xba\xbd\x83\x04\x1b\xa2\x83\x03A\xa1\xd6\x05\xf1\x1b\xc2\xb6\xf0\xa8|\x86;F\xa8H*\x88\xdcv\x9av\xbf\x1fj\xa5=\x19\x8f\xeb8\xf3d\xde\xc8+\r\n(\xff\xf7\xdb\xe2\x15B\xd4\"\xd0\']\xe1y\xfe\x18\xe7p\x88\xadN\xe6\xd9\x8b:\xc6\xdd\'Qn\xff\xbcd\xf53CO\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Trustwave Global Certification Authority O=Trustwave Holdings, Inc.
     * Subject: CN=Trustwave Global Certification Authority O=Trustwave Holdings, Inc.
     * Label: "Trustwave Global Certification Authority"
     * Serial: 1846098327275375458322922162
     * MD5 Fingerprint: f8:1c:18:2d:2f:ba:5f:6d:a1:6c:bc:c7:ab:91:c7:0e
     * SHA1 Fingerprint: 2f:8f:36:4f:e1:58:97:44:21:59:87:a5:2a:9a:d0:69:95:26:7f:b5
     * SHA256 Fingerprint: 97:55:20:15:f5:dd:fc:3c:87:88:c0:06:94:45:55:40:88:94:45:00:84:f1:00:86:70:86:bc:1a:2b:b5:8d:c8
     * -----BEGIN CERTIFICATE-----
     * MIIF2jCCA8KgAwIBAgIMBfcOhtpJ80Y1LrqyMA0GCSqGSIb3DQEBCwUAMIGIMQsw
     * CQYDVQQGEwJVUzERMA8GA1UECAwISWxsaW5vaXMxEDAOBgNVBAcMB0NoaWNhZ28x
     * ITAfBgNVBAoMGFRydXN0d2F2ZSBIb2xkaW5ncywgSW5jLjExMC8GA1UEAwwoVHJ1
     * c3R3YXZlIEdsb2JhbCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTAeFw0xNzA4MjMx
     * OTM0MTJaFw00MjA4MjMxOTM0MTJaMIGIMQswCQYDVQQGEwJVUzERMA8GA1UECAwI
     * SWxsaW5vaXMxEDAOBgNVBAcMB0NoaWNhZ28xITAfBgNVBAoMGFRydXN0d2F2ZSBI
     * b2xkaW5ncywgSW5jLjExMC8GA1UEAwwoVHJ1c3R3YXZlIEdsb2JhbCBDZXJ0aWZp
     * Y2F0aW9uIEF1dGhvcml0eTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIB
     * ALldUShLPDeS0YLOvR29zd24q88KPuFd5dyqCblXAj7mY2Hf8g+CY66j96xz0Xzn
     * swuvCAAJWX/NKSqIk4cXGIDtiLK0thAfLdZfVaITXdHG6wZWiYj+rDKd/VzDBcdu
     * 7oaJuogDnXIhhpCujwOl3J+IKMujkkkP7NAP4m1ET4BqstTnoApTAbqOl5F2brz8
     * 1Ws25kCI1nsvXwXoLG0R8+eyvpJETNKXpP7ScoFDB5zpET71ixpZfR9oWN0EACyW
     * 80OzfpgZdNmcc9kYvkHHNHnZ9GLCQ7mzJ7Aiy/k9UscwR7PJPrhq4ufogXBeQotP
     * JqX+OsIgbrv4Fo7NDKm0G2x2EOFYeUY+VM6AqFcJNykbmROPDMjWLBz7BegIlT1l
     * RtzuzWniTY+HKE40Cz7PFNm73bZQmq131BnW2hqIyE4bJ3XYsgjxroMwuREOzYfw
     * hI0Vcnyh78zyiGG69Gm7DIwLdVcEuE4qFC49DxweMqZiNu5m4iK4BUBjECLzMx10
     * coos9TkpoNPnG4CELcU9402x/RpvumUHO1jsQkUm+9jaJXLE9gCxInm943xZYkqc
     * BW89zubWR2OZxiRvchLIrH+QtAuRcOi35hYQcRfO3gZPSEF9NUqjifLJS3tBEW1n
     * twiYTOURGa5CgNz7kAXU+FDKvuStx8KU1xad5hePrzb7AgMBAAGjQjBAMA8GA1Ud
     * EwEB/wQFMAMBAf8wHQYDVR0OBBYEFJngGWcNYtt2s9o9uFvo/ULSMQ6HMA4GA1Ud
     * DwEB/wQEAwIBBjANBgkqhkiG9w0BAQsFAAOCAgEAmHNw4rDT7TnsTGDZqRKGFx6W
     * 0OhUKDtkLSGm+J1WE2pIPU/HPinbbViDVD2HfSMF1OQc3Og4ZYbFdada2zUFvXfe
     * uyk3QAUHw5RSn8pk3fEbK9xGChACMf1KaA0HZJDmHvUqoai7PF35owgLEQzxPy0Q
     * lG/+4jSHg9bP5Rs1bdID4bANqKCqRieCNqcVtgimQlRXtpla4gt5kNdXElE1GYhB
     * aCXUNxeEFfsBctyV3lImIJgm4nb1J2/6ADtKYdkNy1GTKv0WBpanI5ojSP5RvbbE
     * sLFUzt5sQa0WZ37b/TjNuThOssFgy50X31ieemKyJo90lZvkWx3SD92YHJtZuSPT
     * MaCm/zjdzyBP6VhWOmfD0faZmZ26NraAL4hHT4a/RDqA5Dccprrql5gR0IRiR2Qe
     * qu5AvzSxnI9O4fKSTx+O856X3vOmeWqJcU9LJxdI/uz0UA9PSX3MReO9ekDFQdxh
     * VicGaeVyQYHTtgGJoC86cnn+OjC/QezHYj6RS8fZMXZC+fc8Y+wmjHMMfRod6qh8
     * h6jCJ3zhM0EPz8/8AKAigJ5Kp28AsEFFtyLKaEjFQqKu3R3y4G5OBVixwJAWKqQ9
     * EEC+j2Jjg6mcgn0tAumDMHzLJ8n9HmYAsC7TIS+OMxZsmO0QqAfWzJPP29FpHOTK
     * yeC2nOnOcXHebD8WpHk=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x110\x0f\x06\x03U\x04\x08\x0c\x08Illinois1\x100\x0e\x06\x03U\x04\x07\x0c\x07Chicago1!0\x1f\x06\x03U\x04\n\x0c\x18Trustwave Holdings, Inc.110/\x06\x03U\x04\x03\x0c(Trustwave Global Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb9]Q(K<7\x92\xd1\x82\xce\xbd\x1d\xbd\xcd\xdd\xb8\xab\xcf\n>\xe1]\xe5\xdc\xaa\t\xb9W\x02>\xe6ca\xdf\xf2\x0f\x82c\xae\xa3\xf7\xacs\xd1|\xe7\xb3\x0b\xaf\x08\x00\tY\x7f\xcd)*\x88\x93\x87\x17\x18\x80\xed\x88\xb2\xb4\xb6\x10\x1f-\xd6_U\xa2\x13]\xd1\xc6\xeb\x06V\x89\x88\xfe\xac2\x9d\xfd\\\xc3\x05\xc7n\xee\x86\x89\xba\x88\x03\x9dr!\x86\x90\xae\x8f\x03\xa5\xdc\x9f\x88(\xcb\xa3\x92I\x0f\xec\xd0\x0f\xe2mDO\x80j\xb2\xd4\xe7\xa0\nS\x01\xba\x8e\x97\x91vn\xbc\xfc\xd5k6\xe6@\x88\xd6{/_\x05\xe8,m\x11\xf3\xe7\xb2\xbe\x92DL\xd2\x97\xa4\xfe\xd2r\x81C\x07\x9c\xe9\x11>\xf5\x8b\x1aY}\x1fhX\xdd\x04\x00,\x96\xf3C\xb3~\x98\x19t\xd9\x9cs\xd9\x18\xbeA\xc74y\xd9\xf4b\xc2C\xb9\xb3\'\xb0\"\xcb\xf9=R\xc70G\xb3\xc9>\xb8j\xe2\xe7\xe8\x81p^B\x8bO&\xa5\xfe:\xc2 n\xbb\xf8\x16\x8e\xcd\x0c\xa9\xb4\x1blv\x10\xe1XyF>T\xce\x80\xa8W\t7)\x1b\x99\x13\x8f\x0c\xc8\xd6,\x1c\xfb\x05\xe8\x08\x95=eF\xdc\xee\xcdi\xe2M\x8f\x87(N4\x0b>\xcf\x14\xd9\xbb\xdd\xb6P\x9a\xadw\xd4\x19\xd6\xda\x1a\x88\xc8N\x1b\'u\xd8\xb2\x08\xf1\xae\x830\xb9\x11\x0e\xcd\x87\xf0\x84\x8d\x15r|\xa1\xef\xcc\xf2\x88a\xba\xf4i\xbb\x0c\x8c\x0buW\x04\xb8N*\x14.=\x0f\x1c\x1e2\xa6b6\xeef\xe2\"\xb8\x05@c\x10\"\xf33\x1dtr\x8a,\xf59)\xa0\xd3\xe7\x1b\x80\x84-\xc5=\xe3M\xb1\xfd\x1ao\xbae\x07;X\xecBE&\xfb\xd8\xda%r\xc4\xf6\x00\xb1\"y\xbd\xe3|YbJ\x9c\x05o=\xce\xe6\xd6Gc\x99\xc6$or\x12\xc8\xac\x7f\x90\xb4\x0b\x91p\xe8\xb7\xe6\x16\x10q\x17\xce\xde\x06OHA}5J\xa3\x89\xf2\xc9K{A\x11mg\xb7\x08\x98L\xe5\x11\x19\xaeB\x80\xdc\xfb\x90\x05\xd4\xf8P\xca\xbe\xe4\xad\xc7\xc2\x94\xd7\x16\x9d\xe6\x17\x8f\xaf6\xfb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Buypass Class 2 Root CA O=Buypass AS-983163327
     * Subject: CN=Buypass Class 2 Root CA O=Buypass AS-983163327
     * Label: "Buypass Class 2 Root CA"
     * Serial: 2
     * MD5 Fingerprint: 46:a7:d2:fe:45:fb:64:5a:a8:59:90:9b:78:44:9b:29
     * SHA1 Fingerprint: 49:0a:75:74:de:87:0a:47:fe:58:ee:f6:c7:6b:eb:c6:0b:12:40:99
     * SHA256 Fingerprint: 9a:11:40:25:19:7c:5b:b9:5d:94:e6:3d:55:cd:43:79:08:47:b6:46:b2:3c:df:11:ad:a4:a0:0e:ff:15:fb:48
     * -----BEGIN CERTIFICATE-----
     * MIIFWTCCA0GgAwIBAgIBAjANBgkqhkiG9w0BAQsFADBOMQswCQYDVQQGEwJOTzEd
     * MBsGA1UECgwUQnV5cGFzcyBBUy05ODMxNjMzMjcxIDAeBgNVBAMMF0J1eXBhc3Mg
     * Q2xhc3MgMiBSb290IENBMB4XDTEwMTAyNjA4MzgwM1oXDTQwMTAyNjA4MzgwM1ow
     * TjELMAkGA1UEBhMCTk8xHTAbBgNVBAoMFEJ1eXBhc3MgQVMtOTgzMTYzMzI3MSAw
     * HgYDVQQDDBdCdXlwYXNzIENsYXNzIDIgUm9vdCBDQTCCAiIwDQYJKoZIhvcNAQEB
     * BQADggIPADCCAgoCggIBANfHXvfBB9R3+0Mh9PT1aeTuMgHbo4Yf5FkNuud1g1Lr
     * 6hxhFUi7HQfKjK6w3Jad6sNgkoaCKHOcVgb/S2TwDCo3SbXlzwx87vFKu3MwZfPV
     * L4O2fuPn9Z6rYPnT8Z2SdIrkHJasW4DptfQxh6NR/Md+oW+OU3fUl8FVM5I+GC91
     * 1K2GScuVr1QGbNgGE41b/+EmGVnAJLqBcXmQRFBoJJRfuLMR8SlBYaNByyM21cHx
     * MlAQTn/0hpPshNOOvEu/XAFOBz3cFIqUCqTqc/sLUegTBxj6DvEr0VQVfTzh97QZ
     * QmdiXnfgolXsttlpF9U6r0TtSsWe5HonfOV116rLJeffawrbD02TTqigzXsu8lkB
     * arcNuAeBfos4GzjmCleZPe4h6KP1DBbdi+w0jpwqHAAVF41og9JwnxgIzRFo1clr
     * Us3ERo/ctfPYV3Me6ZQ5BL/T3jjetFPsaRyifsSP5BtwrfKi+fv3FmRmaZ9JUaLi
     * FRhnBkp/1Wy1TbMz4GHrXb7pmA8y1x1LPC5aAVKRCfLf6o3YBkBjqhHk/sM3nhRS
     * P/TizPJhk9H9Z2vXUq6/aKtAQ6BXNVN48FP4YUIHZMbXb5tMOA1jrGKvNouicwoN
     * 9SG9dKpN6nIDSdvHXx1iY8f93ZHsM+71bbRuMGjeyNYmsHVee7QHIJihdjK4TWxP
     * AgMBAAGjQjBAMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFMmAd+BikoL1Rpzz
     * uvdMw964o605MA4GA1UdDwEB/wQEAwIBBjANBgkqhkiG9w0BAQsFAAOCAgEAU18h
     * 9bqwOlI5LJKwbADJ784g7wbylp7ppHR/ehb8t/W2+xUbP6umwHJdELFx7rxP462s
     * A20ucS6vxOOto70MEae0/0qyexAQH6dXQbLArvQsWdZHEIjzIVEpMMpghq9Gqx3t
     * OluwlN5E40EIosHsHdb9T7bWR9AUC8rmyrV7d35BH16Dx7aMOZawP5aBQW9gkOLo
     * +fsicdl9sz1Gv7SEr5AcD48Saq/v7h56rgJKihcrdv6sVIkkLE8/trKnToyokZf7
     * KcZ7XC25y2a2t6hbElGFtQl+Ynhw/qlqYLYdDnkM/crqJIByw5c/8nerQyIKx+u2
     * DISCLIBrQYoIwOula9+ZEsuK1V6ADJHgJgg2SMX6OBE1/yWDLfJ6v9r9jv6ly0Us
     * H8SIU653DtmadsWOLB2jutXsMq7Aqqz30XpN69QH4kj3Io6wpJ9qzo6ysmD0oyLQ
     * I+uUWnpp3Q+/QFesa1lQ2aOZ4W7+jQF5JyMV3pKdewlNWudLSDBaGOYKbeaP4NK7
     * 5t98biGCwWg5TbSYWGZizEqQXsP6JwSxeRV0mcy+rSDeJmAc61ZRpqPq5KM/p/9h
     * 3PFaTWwyI0PurKju7koSCTxdccK+efrCh2gdC/1cacwG0Jp9VJkqyTkaGa9LKkPz
     * Y11aWOIv4x3kqdbQCtCev9eBCfHJxyYNrJgWVqA=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02NO1\x1d0\x1b\x06\x03U\x04\n\x0c\x14Buypass AS-9831633271 0\x1e\x06\x03U\x04\x03\x0c\x17Buypass Class 2 Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xd7\xc7^\xf7\xc1\x07\xd4w\xfbC!\xf4\xf4\xf5i\xe4\xee2\x01\xdb\xa3\x86\x1f\xe4Y\r\xba\xe7u\x83R\xeb\xea\x1ca\x15H\xbb\x1d\x07\xca\x8c\xae\xb0\xdc\x96\x9d\xea\xc3`\x92\x86\x82(s\x9cV\x06\xffKd\xf0\x0c*7I\xb5\xe5\xcf\x0c|\xee\xf1J\xbbs0e\xf3\xd5/\x83\xb6~\xe3\xe7\xf5\x9e\xab`\xf9\xd3\xf1\x9d\x92t\x8a\xe4\x1c\x96\xac[\x80\xe9\xb5\xf41\x87\xa3Q\xfc\xc7~\xa1o\x8eSw\xd4\x97\xc1U3\x92>\x18/u\xd4\xad\x86I\xcb\x95\xafT\x06l\xd8\x06\x13\x8d[\xff\xe1&\x19Y\xc0$\xba\x81qy\x90DPh$\x94_\xb8\xb3\x11\xf1)Aa\xa3A\xcb#6\xd5\xc1\xf12P\x10N\x7f\xf4\x86\x93\xec\x84\xd3\x8e\xbcK\xbf\\\x01N\x07=\xdc\x14\x8a\x94\n\xa4\xeas\xfb\x0bQ\xe8\x13\x07\x18\xfa\x0e\xf1+\xd1T\x15}<\xe1\xf7\xb4\x19Bgb^w\xe0\xa2U\xec\xb6\xd9i\x17\xd5:\xafD\xedJ\xc5\x9e\xe4z\'|\xe5u\xd7\xaa\xcb%\xe7\xdfk\n\xdb\x0fM\x93N\xa8\xa0\xcd{.\xf2Y\x01j\xb7\r\xb8\x07\x81~\x8b8\x1b8\xe6\nW\x99=\xee!\xe8\xa3\xf5\x0c\x16\xdd\x8b\xec4\x8e\x9c*\x1c\x00\x15\x17\x8dh\x83\xd2p\x9f\x18\x08\xcd\x11h\xd5\xc9kR\xcd\xc4F\x8f\xdc\xb5\xf3\xd8Ws\x1e\xe9\x949\x04\xbf\xd3\xde8\xde\xb4S\xeci\x1c\xa2~\xc4\x8f\xe4\x1bp\xad\xf2\xa2\xf9\xfb\xf7\x16dfi\x9fIQ\xa2\xe2\x15\x18g\x06J\x7f\xd5l\xb5M\xb33\xe0a\xeb]\xbe\xe9\x98\x0f2\xd7\x1dK<.Z\x01R\x91\t\xf2\xdf\xea\x8d\xd8\x06@c\xaa\x11\xe4\xfe\xc37\x9e\x14R?\xf4\xe2\xcc\xf2a\x93\xd1\xfdgk\xd7R\xae\xbfh\xab@C\xa0W5Sx\xf0S\xf8aB\x07d\xc6\xd7o\x9bL8\rc\xacb\xaf6\x8b\xa2s\n\r\xf5!\xbdt\xaaM\xear\x03I\xdb\xc7_\x1dbc\xc7\xfd\xdd\x91\xec3\xee\xf5m\xb4n0h\xde\xc8\xd6&\xb0u^{\xb4\x07 \x98\xa1v2\xb8MlO\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=ACCVRAIZ1 O=ACCV OU=PKIACCV
     * Subject: CN=ACCVRAIZ1 O=ACCV OU=PKIACCV
     * Label: "ACCVRAIZ1"
     * Serial: 6828503384748696800
     * MD5 Fingerprint: d0:a0:5a:ee:05:b6:09:94:21:a1:7d:f1:b2:29:82:02
     * SHA1 Fingerprint: 93:05:7a:88:15:c6:4f:ce:88:2f:fa:91:16:52:28:78:bc:53:64:17
     * SHA256 Fingerprint: 9a:6e:c0:12:e1:a7:da:9d:be:34:19:4d:47:8a:d7:c0:db:18:22:fb:07:1d:f1:29:81:49:6e:d1:04:38:41:13
     * -----BEGIN CERTIFICATE-----
     * MIIH0zCCBbugAwIBAgIIXsO3pkN/pOAwDQYJKoZIhvcNAQEFBQAwQjESMBAGA1UE
     * AwwJQUNDVlJBSVoxMRAwDgYDVQQLDAdQS0lBQ0NWMQ0wCwYDVQQKDARBQ0NWMQsw
     * CQYDVQQGEwJFUzAeFw0xMTA1MDUwOTM3MzdaFw0zMDEyMzEwOTM3MzdaMEIxEjAQ
     * BgNVBAMMCUFDQ1ZSQUlaMTEQMA4GA1UECwwHUEtJQUNDVjENMAsGA1UECgwEQUND
     * VjELMAkGA1UEBhMCRVMwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCb
     * qau/YUqXry+XZpp0X9DZlv3P4uRm7x8fRzPCRKPfmt4ftVTdFXxpNRFvu8gMjmoY
     * HtiP2Ra8EEg2XPBjs5BaXCQ316PWywlxufEBcoSwfdtNgM3802/J+Nq2DoLSRYWo
     * G2ioPej0RGy9ocLLA76MPhMAhN9KSMDjIgro6TenGEyxCQ0jVn8ETdkXhBilyNpA
     * lHPrzg5XPAOBOp0KoVdDaaxXbXmQeOW1tDvYvEyNKKGno6e6Ak4l0Squ7a4DIrhr
     * IA8wKFSVf+DuzgpmndFALW4ir50awQUZ0m/A8p/4e7MCQvtQqR0tkw8jq8bBD5L/
     * 0KIV9VMJcRz/RROE5iZe+OCIHAr8Fraocwa48GOEAqDGWuzndN9wrqODJerWx5eH
     * k6fGioozl2A3ED6XPm4pFdahD9GILBKfb6qkxkLrQaLjlUPTAYVtjrs78yM2x/47
     * 4KElB0iryYl0/wiPgL/AlmXz7uxLaL2diMMxs0Dx6M/2OLuc5NF/1OVYm3z61PMO
     * m3WR5LpSLhl+0fXNWhn8ugb2+1KoS5kE3fj5tItQo05iifCHJPqDQsGH+tUtKSpa
     * cXpkatcnYGMN285J9Y0fkIkyF/hzQ7jSWpOGYdbhdQrqeWZ2iE9x6wQl1gpaepPl
     * uUsXQA+xtrn13k/c4LOsOxFwYIRKQ26ZIMApcQrAZQIDAQABo4ICyzCCAscwfQYI
     * KwYBBQUHAQEEcTBvMEwGCCsGAQUFBzAChkBodHRwOi8vd3d3LmFjY3YuZXMvZmls
     * ZWFkbWluL0FyY2hpdm9zL2NlcnRpZmljYWRvcy9yYWl6YWNjdjEuY3J0MB8GCCsG
     * AQUFBzABhhNodHRwOi8vb2NzcC5hY2N2LmVzMB0GA1UdDgQWBBTSh7Tj3zcnk1X2
     * VuqB5TbMjB4/vTAPBgNVHRMBAf8EBTADAQH/MB8GA1UdIwQYMBaAFNKHtOPfNyeT
     * VfZW6oHlNsyMHj+9MIIBcwYDVR0gBIIBajCCAWYwggFiBgRVHSAAMIIBWDCCASIG
     * CCsGAQUFBwICMIIBFB6CARAAQQB1AHQAbwByAGkAZABhAGQAIABkAGUAIABDAGUA
     * cgB0AGkAZgBpAGMAYQBjAGkA8wBuACAAUgBhAO0AegAgAGQAZQAgAGwAYQAgAEEA
     * QwBDAFYAIAAoAEEAZwBlAG4AYwBpAGEAIABkAGUAIABUAGUAYwBuAG8AbABvAGcA
     * 7QBhACAAeQAgAEMAZQByAHQAaQBmAGkAYwBhAGMAaQDzAG4AIABFAGwAZQBjAHQA
     * cgDzAG4AaQBjAGEALAAgAEMASQBGACAAUQA0ADYAMAAxADEANQA2AEUAKQAuACAA
     * QwBQAFMAIABlAG4AIABoAHQAdABwADoALwAvAHcAdwB3AC4AYQBjAGMAdgAuAGUA
     * czAwBggrBgEFBQcCARYkaHR0cDovL3d3dy5hY2N2LmVzL2xlZ2lzbGFjaW9uX2Mu
     * aHRtMFUGA1UdHwROMEwwSqBIoEaGRGh0dHA6Ly93d3cuYWNjdi5lcy9maWxlYWRt
     * aW4vQXJjaGl2b3MvY2VydGlmaWNhZG9zL3JhaXphY2N2MV9kZXIuY3JsMA4GA1Ud
     * DwEB/wQEAwIBBjAXBgNVHREEEDAOgQxhY2N2QGFjY3YuZXMwDQYJKoZIhvcNAQEF
     * BQADggIBAJcxAp/n/UNnSEQU5CmH7UwoZtCPNdpNYbdKl02125DgBS4OxnnQ8pdp
     * D70ER9m+27Up2pvZrqmZ1dM8MJP1jaGo/AaNRPTKFpV8M9xii6g3+CfYCS0b78gU
     * JyCpZET/LtZ1qmxNYEAZSUNUY9rizLpm5U9EelvZaoErQNV/+QEnWCzI7UiRfD+m
     * AM/EKXMRNt6GGT6d7hmKG9Ww7Y49nCrADdg9ZuM8Db3VlFzi4qc1GwQA9j9ajepD
     * vV+JHanBsMyZ4k0ACtrJJ1vnE5Bc5PUzolVt3OAJTS+xJlsndQAJxGJ3KQhfnlms
     * tn6tn1QwIgPBHnFk/vk4CpYY3QIUrCPLBhwepH2NDd4nQeit2hW3sCPdK6jT2iWH
     * 7ehVRE2I9DZ+hJp4rPcOVkkO1jMl1oRQQmwgEh0q1b688nCBpHBgvgW1m54ERL5h
     * I6zppSSMEYCUWqKiuUnSwdzRp+0xESyeGabu4VXhwOrPDYTkF7eifKXeVSUG7szA
     * h1xA2syVP1XgNce4hL60Xc16gwFy7ofmXx2utYXGJt/mwZrpHgJHnyqobalbz+xF
     * d3+YJ5oyXSrjhO7FmGYvliAd3djDJ9ew+f7Zfc3Qn48LFFhRny+Lwzgt3uiP1o2H
     * pPVWQxaZLPSkVrQ0uGE3ycJYgBugl6H8WY3pEfbRD0tVNEYqi4Y7
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x120\x10\x06\x03U\x04\x03\x0c\tACCVRAIZ11\x100\x0e\x06\x03U\x04\x0b\x0c\x07PKIACCV1\r0\x0b\x06\x03U\x04\n\x0c\x04ACCV1\x0b0\t\x06\x03U\x04\x06\x13\x02ES",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x9b\xa9\xab\xbfaJ\x97\xaf/\x97f\x9at_\xd0\xd9\x96\xfd\xcf\xe2\xe4f\xef\x1f\x1fG3\xc2D\xa3\xdf\x9a\xde\x1f\xb5T\xdd\x15|i5\x11o\xbb\xc8\x0c\x8ej\x18\x1e\xd8\x8f\xd9\x16\xbc\x10H6\\\xf0c\xb3\x90Z\\$7\xd7\xa3\xd6\xcb\tq\xb9\xf1\x01r\x84\xb0}\xdbM\x80\xcd\xfc\xd3o\xc9\xf8\xda\xb6\x0e\x82\xd2E\x85\xa8\x1bh\xa8=\xe8\xf4Dl\xbd\xa1\xc2\xcb\x03\xbe\x8c>\x13\x00\x84\xdfJH\xc0\xe3\"\n\xe8\xe97\xa7\x18L\xb1\t\r#V\x7f\x04M\xd9\x17\x84\x18\xa5\xc8\xda@\x94s\xeb\xce\x0eW<\x03\x81:\x9d\n\xa1WCi\xacWmy\x90x\xe5\xb5\xb4;\xd8\xbcL\x8d(\xa1\xa7\xa3\xa7\xba\x02N%\xd1*\xae\xed\xae\x03\"\xb8k \x0f0(T\x95\x7f\xe0\xee\xce\nf\x9d\xd1@-n\"\xaf\x9d\x1a\xc1\x05\x19\xd2o\xc0\xf2\x9f\xf8{\xb3\x02B\xfbP\xa9\x1d-\x93\x0f#\xab\xc6\xc1\x0f\x92\xff\xd0\xa2\x15\xf5S\tq\x1c\xffE\x13\x84\xe6&^\xf8\xe0\x88\x1c\n\xfc\x16\xb6\xa8s\x06\xb8\xf0c\x84\x02\xa0\xc6Z\xec\xe7t\xdfp\xae\xa3\x83%\xea\xd6\xc7\x97\x87\x93\xa7\xc6\x8a\x8a3\x97`7\x10>\x97>n)\x15\xd6\xa1\x0f\xd1\x88,\x12\x9fo\xaa\xa4\xc6B\xebA\xa2\xe3\x95C\xd3\x01\x85m\x8e\xbb;\xf3#6\xc7\xfe;\xe0\xa1%\x07H\xab\xc9\x89t\xff\x08\x8f\x80\xbf\xc0\x96e\xf3\xee\xecKh\xbd\x9d\x88\xc31\xb3@\xf1\xe8\xcf\xf68\xbb\x9c\xe4\xd1\x7f\xd4\xe5X\x9b|\xfa\xd4\xf3\x0e\x9bu\x91\xe4\xbaR.\x19~\xd1\xf5\xcdZ\x19\xfc\xba\x06\xf6\xfbR\xa8K\x99\x04\xdd\xf8\xf9\xb4\x8bP\xa3Nb\x89\xf0\x87$\xfa\x83B\xc1\x87\xfa\xd5-)*Zqzdj\xd7\'`c\r\xdb\xceI\xf5\x8d\x1f\x90\x892\x17\xf8sC\xb8\xd2Z\x93\x86a\xd6\xe1u\n\xeayfv\x88Oq\xeb\x04%\xd6\nZz\x93\xe5\xb9K\x17@\x0f\xb1\xb6\xb9\xf5\xdeO\xdc\xe0\xb3\xac;\x11p`\x84JCn\x99 \xc0)q\n\xc0e\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=UCA Global G2 Root O=UniTrust
     * Subject: CN=UCA Global G2 Root O=UniTrust
     * Label: "UCA Global G2 Root"
     * Serial: 124779693093741543919145257850076631279
     * MD5 Fingerprint: 80:fe:f0:c4:4a:f0:5c:62:32:9f:1c:ba:78:a9:50:f8
     * SHA1 Fingerprint: 28:f9:78:16:19:7a:ff:18:25:18:aa:44:fe:c1:a0:ce:5c:b6:4c:8a
     * SHA256 Fingerprint: 9b:ea:11:c9:76:fe:01:47:64:c1:be:56:a6:f9:14:b5:a5:60:31:7a:bd:99:88:39:33:82:e5:16:1a:a0:49:3c
     * -----BEGIN CERTIFICATE-----
     * MIIFRjCCAy6gAwIBAgIQXd+x2lqj7V2+WmUgZQOQ7zANBgkqhkiG9w0BAQsFADA9
     * MQswCQYDVQQGEwJDTjERMA8GA1UECgwIVW5pVHJ1c3QxGzAZBgNVBAMMElVDQSBH
     * bG9iYWwgRzIgUm9vdDAeFw0xNjAzMTEwMDAwMDBaFw00MDEyMzEwMDAwMDBaMD0x
     * CzAJBgNVBAYTAkNOMREwDwYDVQQKDAhVbmlUcnVzdDEbMBkGA1UEAwwSVUNBIEds
     * b2JhbCBHMiBSb290MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAxeYr
     * b3zvJgUno4Ek2m/LAfmZmqkywiKHYUGRO8vDaBsGxUypK8FnFyIdK+35KYmToni9
     * kmugow2ifsqTs6bRjDXVdfkX9s9FxeV67HeToI8jrg4aA3++1NDtLnurRiNb/yzm
     * VHqUwCoV8MmNsHo7JOHXaOIxPAYzRrZUEaalLyJUKlgNAQLx+hVRZ2zA+te2G3/R
     * VogvGjqNO7uCEeBHANBSh6v7hn4PJGtAnTRnvI3HLYZveT6OqTwXS3+wmeOwcWDc
     * C/Vkw85DvG1xudLeJ1uK6NjGruFZfc8oLTW4lVYa8bJYS7cSN8h8s+1LgOGN+jIj
     * tm+3SJUIsUROhYw6AlQgL9+/V087OpAh18EmNVQg7Mc/R+zvWr9LesGtOxdQXGLY
     * D0tK3Cv6brxzks3sx1DoQZbXqX5t2Okdj4q1uViSukqSKwxW/YDrCPBeKW4bHAyv
     * j5OJrdu9o54hyokZ7N+1wxrrFv54NkzWbtA+FxyQF2smuvt6L78RHBgOLXMDj6Dl
     * NaBa4kx1HXHhOThTeEDMg5PXCp6dW4+K5OXgSORIskfNTip1KnvyIvbJvgmRlld6
     * iIis7nCs+dwp4wwcOxJORNanTrAmyPPZGpeRaOrvjUYG0lZFWJo8DA+DuAUlwznP
     * O6Q0ibd5Ei9Hxeepl2n8pndntd978XplFeRhVmUCAwEAAaNCMEAwDgYDVR0PAQH/
     * BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFIHEjMz15DD/pQwIX4wV
     * ZyF0Ad/fMA0GCSqGSIb3DQEBCwUAA4ICAQATZSL1jiutROTL/7lo5sOASD0Ee/oj
     * L3rtNtqyzm325p7lX1iPyzcyochltq44PTUbPrw7tgTQvPlJ9Zv3hcU2tsu8+Mg5
     * 1eRfB70VVJd0ysrtT7q6ZHafgbiERUlMjW+i67HM0cOU2kTC5uLqGOiiHycFutfl
     * 1qnN3e92mI0ADs0b+gO3joBYDic/UvuUospeZcnWhNq5NXHzJsBPd+aBJ9J3O5oU
     * b3n09tDh05S60FdRvScFDcH9yBIw7m+NESsIndTUv4BFFJqIRNow6rSn4+7vW4LV
     * PtateJLbXDzz2K36uGt/xDYotgIVilQsnLAXc47QN6MUPJiVAAwpBVueSUmxX8fj
     * y88nZY41F7dXyDDZQVu5FLbowg+UMaeUmMxq67XhJ/UQqAHojhJi6IjMtX9Gl8Cb
     * EGY4GjZGXyJoPd/JxhMnq1MGrKI8hgZlb7F+sSlEmqO6SWkoaY/X5V+tBIZkbxqg
     * DMUIYs6Ao9Dz7GjevjPHF1t/gMRMTLGmhIrDO7gJzRSBuhjjVFc2/tsvfEehOjPI
     * +Vg7RE+xygKJBJYoaMVLuCaJu9YzL1DV/pqJuhgyklTGW+Cd+V7lDSKb9triyCGy
     * YiGqhkCyLmTTX8jjfhFnRR8F/uOi77Oos/N9j/gMHyIfLXC0uAE0djAA5SN4p1bX
     * UB+K+wb1whnw0A==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CN1\x110\x0f\x06\x03U\x04\n\x0c\x08UniTrust1\x1b0\x19\x06\x03U\x04\x03\x0c\x12UCA Global G2 Root",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc5\xe6+o|\xef&\x05\'\xa3\x81$\xdao\xcb\x01\xf9\x99\x9a\xa92\xc2\"\x87aA\x91;\xcb\xc3h\x1b\x06\xc5L\xa9+\xc1g\x17\"\x1d+\xed\xf9)\x89\x93\xa2x\xbd\x92k\xa0\xa3\r\xa2~\xca\x93\xb3\xa6\xd1\x8c5\xd5u\xf9\x17\xf6\xcfE\xc5\xe5z\xecw\x93\xa0\x8f#\xae\x0e\x1a\x03\x7f\xbe\xd4\xd0\xed.{\xabF#[\xff,\xe6Tz\x94\xc0*\x15\xf0\xc9\x8d\xb0z;$\xe1\xd7h\xe21<\x063F\xb6T\x11\xa6\xa5/\"T*X\r\x01\x02\xf1\xfa\x15Qgl\xc0\xfa\xd7\xb6\x1b\x7f\xd1V\x88/\x1a:\x8d;\xbb\x82\x11\xe0G\x00\xd0R\x87\xab\xfb\x86~\x0f$k@\x9d4g\xbc\x8d\xc7-\x86oy>\x8e\xa9<\x17K\x7f\xb0\x99\xe3\xb0q`\xdc\x0b\xf5d\xc3\xceC\xbcmq\xb9\xd2\xde\'[\x8a\xe8\xd8\xc6\xae\xe1Y}\xcf(-5\xb8\x95V\x1a\xf1\xb2XK\xb7\x127\xc8|\xb3\xedK\x80\xe1\x8d\xfa2#\xb6o\xb7H\x95\x08\xb1DN\x85\x8c:\x02T /\xdf\xbfWO;:\x90!\xd7\xc1&5T \xec\xc7?G\xec\xefZ\xbfKz\xc1\xad;\x17P\\b\xd8\x0fKJ\xdc+\xfan\xbcs\x92\xcd\xec\xc7P\xe8A\x96\xd7\xa9~m\xd8\xe9\x1d\x8f\x8a\xb5\xb9X\x92\xbaJ\x92+\x0cV\xfd\x80\xeb\x08\xf0^)n\x1b\x1c\x0c\xaf\x8f\x93\x89\xad\xdb\xbd\xa3\x9e!\xca\x89\x19\xec\xdf\xb5\xc3\x1a\xeb\x16\xfex6L\xd6n\xd0>\x17\x1c\x90\x17k&\xba\xfbz/\xbf\x11\x1c\x18\x0e-s\x03\x8f\xa0\xe55\xa0Z\xe2Lu\x1dq\xe198Sx@\xcc\x83\x93\xd7\n\x9e\x9d[\x8f\x8a\xe4\xe5\xe0H\xe4H\xb2G\xcdN*u*{\xf2\"\xf6\xc9\xbe\t\x91\x96Wz\x88\x88\xac\xeep\xac\xf9\xdc)\xe3\x0c\x1c;\x12ND\xd6\xa7N\xb0&\xc8\xf3\xd9\x1a\x97\x91h\xea\xef\x8dF\x06\xd2VEX\x9a<\x0c\x0f\x83\xb8\x05%\xc39\xcf;\xa44\x89\xb7y\x12/G\xc5\xe7\xa9\x97i\xfc\xa6wg\xb5\xdf{\xf1ze\x15\xe4aVe\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Hellenic Academic and Research Institutions RootCA 2015 O=Hellenic Academic and Research Institutions Cert. Authority
     * Subject: CN=Hellenic Academic and Research Institutions RootCA 2015 O=Hellenic Academic and Research Institutions Cert. Authority
     * Label: "Hellenic Academic and Research Institutions RootCA 2015"
     * Serial: 0
     * MD5 Fingerprint: ca:ff:e2:db:03:d9:cb:4b:e9:0f:ad:84:fd:7b:18:ce
     * SHA1 Fingerprint: 01:0c:06:95:a6:98:19:14:ff:bf:5f:c6:b0:b6:95:ea:29:e9:12:a6
     * SHA256 Fingerprint: a0:40:92:9a:02:ce:53:b4:ac:f4:f2:ff:c6:98:1c:e4:49:6f:75:5e:6d:45:fe:0b:2a:69:2b:cd:52:52:3f:36
     * -----BEGIN CERTIFICATE-----
     * MIIGCzCCA/OgAwIBAgIBADANBgkqhkiG9w0BAQsFADCBpjELMAkGA1UEBhMCR1Ix
     * DzANBgNVBAcTBkF0aGVuczFEMEIGA1UEChM7SGVsbGVuaWMgQWNhZGVtaWMgYW5k
     * IFJlc2VhcmNoIEluc3RpdHV0aW9ucyBDZXJ0LiBBdXRob3JpdHkxQDA+BgNVBAMT
     * N0hlbGxlbmljIEFjYWRlbWljIGFuZCBSZXNlYXJjaCBJbnN0aXR1dGlvbnMgUm9v
     * dENBIDIwMTUwHhcNMTUwNzA3MTAxMTIxWhcNNDAwNjMwMTAxMTIxWjCBpjELMAkG
     * A1UEBhMCR1IxDzANBgNVBAcTBkF0aGVuczFEMEIGA1UEChM7SGVsbGVuaWMgQWNh
     * ZGVtaWMgYW5kIFJlc2VhcmNoIEluc3RpdHV0aW9ucyBDZXJ0LiBBdXRob3JpdHkx
     * QDA+BgNVBAMTN0hlbGxlbmljIEFjYWRlbWljIGFuZCBSZXNlYXJjaCBJbnN0aXR1
     * dGlvbnMgUm9vdENBIDIwMTUwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoIC
     * AQDC+Kk/G4n8PDwEXT2QNrCROnk8ZlrvbTkBSRq0t89/TSNTt5AA4xMqKKYx8ZEA
     * 4yjsriFBzh/a/X0SWwGDD7mwX5nh8hKDgE0GPt+sr+ehiGsxr/CL0BgzuNtFajT0
     * AoAkKAoCFZVedioNmToUW/bLy1O8E00BiDeUJRtCvCLYjqOWXjrZMts+6PAQZe10
     * 4S+nfK8nNLspfZu2zwnI5dMK/IhlZXQK3HMcXM1AsRzUtoSMTFDPaI6oWa7CJ06C
     * ojXdFPQf/7J31Ycvqm59JCfnxssm5uX+Zwdj2EUN3TpZZTlYepKZcj2chF6IIbjV
     * 9Cz82XBST3i4vTwri5WY9bPRaM8gFH5MXF/ni+X1NYEZN9cRCLdmvtNKzoNXADrD
     * gfgXy5I2XdGj2HUb4Ysn6npIQf1FGQatJ5lOwXBH3bWfgVMS5bGMSF0xQxfjjMZ6
     * Y5ZLKTBOhE5iGV48zpeQpX8B653g+IuJ3SWYPZK2fu/Z8VFRfS0myGlZYeCsargq
     * NhEEelC9MoS+L9xy1dcdFkfkR2YgP/SWxa+OAXqlD3pk9Q0Yh9muiNX6hME6wGko
     * LfINaFGq46V3xqSQDqE3izEjR8EJCOtu93ib14L8hCCZSRm2Ekax+0VVFqmjZayc
     * Bw/qa9wfLgZy7IaIEuQt218FL+TwA9MmM+eAws1CoRc0CwIDAQABo0IwQDAPBgNV
     * HRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAdBgNVHQ4EFgQUcRVnyMjJvXVd
     * ctA4GGqd83EkVAswDQYJKoZIhvcNAQELBQADggIBAHW7bVRLqhBYRjTyYtcWNl0I
     * XtVsyIe9tC5G8jH4fOpCtZMWVdyhDBKg2mF+D1hYc2Ryx+hFjtyp8iY/xnmMsVMI
     * M4GwVhO+5lFc2JsKT0ucVlMC6U/2DWDqTUJV6HwbISHTGzrMd/K4kPFox/la/vot
     * 9L/J9UUbzjgQKjeKeaO04wlshYaT/4mWJ3iBj2fjRnRUjtkNaeJK9E10A/+yd+2V
     * Z5fkscWrv2oj6NSU4kQoYsRL4vDY4ilrGnB+JGGTe08DMiUNRSQrlrRGar9KC/ea
     * j8GsGsVn82800vpzY4zvFrCopEYq+OsS7HK07/grfoxSwIuEVPkvPuNVqNxmsdnh
     * X9izjFk0WaSrT2y7HxjbdavYy5LNlDhhDgcGH0tGEPEVvo2FXDtKK4F5D7Rpn0lQ
     * l033DlZdwJVqwjbDG2jJ9SrcR5q+ss7FJej6A7na+RZukYT1HCjI/CbM1xyQVqdf
     * bzoEvM14iQuODy+jqk+iGxI9FghAD/FGTNeqewjBCvVtJ94Cj8rDtSvK6evIIVM4
     * pcw72Hc3MKJP2W/R8kCtQXoXxdZKNYm3QdV8hn9VTYNKpXMgwDqvkPGaJI7ZjnHK
     * e7iG2rKPmT4dEw0SEe7Uq/DpFXYC5ODfqiAeW2GFZECpkJcNrVPSWh2HagCXZWK0
     * vm9qp/UsQu0yrbYhnr68
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GR1\x0f0\r\x06\x03U\x04\x07\x13\x06Athens1D0B\x06\x03U\x04\n\x13;Hellenic Academic and Research Institutions Cert. Authority1@0>\x06\x03U\x04\x03\x137Hellenic Academic and Research Institutions RootCA 2015",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc2\xf8\xa9?\x1b\x89\xfc<<\x04]=\x906\xb0\x91:y<fZ\xefm9\x01I\x1a\xb4\xb7\xcf\x7fM#S\xb7\x90\x00\xe3\x13*(\xa61\xf1\x91\x00\xe3(\xec\xae!A\xce\x1f\xda\xfd}\x12[\x01\x83\x0f\xb9\xb0_\x99\xe1\xf2\x12\x83\x80M\x06>\xdf\xac\xaf\xe7\xa1\x88k1\xaf\xf0\x8b\xd0\x183\xb8\xdbEj4\xf4\x02\x80$(\n\x02\x15\x95^v*\r\x99:\x14[\xf6\xcb\xcbS\xbc\x13M\x01\x887\x94%\x1bB\xbc\"\xd8\x8e\xa3\x96^:\xd92\xdb>\xe8\xf0\x10e\xedt\xe1/\xa7|\xaf\'4\xbb)}\x9b\xb6\xcf\t\xc8\xe5\xd3\n\xfc\x88eet\n\xdcs\x1c\\\xcd@\xb1\x1c\xd4\xb6\x84\x8cLP\xcfh\x8e\xa8Y\xae\xc2\'N\x82\xa25\xdd\x14\xf4\x1f\xff\xb2w\xd5\x87/\xaan}$\'\xe7\xc6\xcb&\xe6\xe5\xfeg\x07c\xd8E\r\xdd:Ye9Xz\x92\x99r=\x9c\x84^\x88!\xb8\xd5\xf4,\xfc\xd9pROx\xb8\xbd<+\x8b\x95\x98\xf5\xb3\xd1h\xcf \x14~L\\_\xe7\x8b\xe5\xf55\x81\x197\xd7\x11\x08\xb7f\xbe\xd3J\xce\x83W\x00:\xc3\x81\xf8\x17\xcb\x926]\xd1\xa3\xd8u\x1b\xe1\x8b\'\xeazHA\xfdE\x19\x06\xad\'\x99N\xc1pG\xdd\xb5\x9f\x81S\x12\xe5\xb1\x8cH]1C\x17\xe3\x8c\xc6zc\x96K)0N\x84Nb\x19^<\xce\x97\x90\xa5\x7f\x01\xeb\x9d\xe0\xf8\x8b\x89\xdd%\x98=\x92\xb6~\xef\xd9\xf1QQ}-&\xc8iYa\xe0\xacj\xb8*6\x11\x04zP\xbd2\x84\xbe/\xdcr\xd5\xd7\x1d\x16G\xe4Gf ?\xf4\x96\xc5\xaf\x8e\x01z\xa5\x0fzd\xf5\r\x18\x87\xd9\xae\x88\xd5\xfa\x84\xc1:\xc0i(-\xf2\rhQ\xaa\xe3\xa5w\xc6\xa4\x90\x0e\xa17\x8b1#G\xc1\t\x08\xebn\xf7x\x9b\xd7\x82\xfc\x84 \x99I\x19\xb6\x12F\xb1\xfbEU\x16\xa9\xa3e\xac\x9c\x07\x0f\xeak\xdc\x1f.\x06r\xec\x86\x88\x12\xe4-\xdb_\x05/\xe4\xf0\x03\xd3&3\xe7\x80\xc2\xcdB\xa1\x174\x0b\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=SZAFIR ROOT CA2 O=Krajowa Izba Rozliczeniowa S.A.
     * Subject: CN=SZAFIR ROOT CA2 O=Krajowa Izba Rozliczeniowa S.A.
     * Label: "SZAFIR ROOT CA2"
     * Serial: 357043034767186914217277344587386743377558296292
     * MD5 Fingerprint: 11:64:c1:89:b0:24:b1:8c:b1:07:7e:89:9e:51:9e:99
     * SHA1 Fingerprint: e2:52:fa:95:3f:ed:db:24:60:bd:6e:28:f3:9c:cc:cf:5e:b3:3f:de
     * SHA256 Fingerprint: a1:33:9d:33:28:1a:0b:56:e5:57:d3:d3:2b:1c:e7:f9:36:7e:b0:94:bd:5f:a7:2a:7e:50:04:c8:de:d7:ca:fe
     * -----BEGIN CERTIFICATE-----
     * MIIDcjCCAlqgAwIBAgIUPopdB+xV0jLVt+O2XwHrLdzk1uQwDQYJKoZIhvcNAQEL
     * BQAwUTELMAkGA1UEBhMCUEwxKDAmBgNVBAoMH0tyYWpvd2EgSXpiYSBSb3psaWN6
     * ZW5pb3dhIFMuQS4xGDAWBgNVBAMMD1NaQUZJUiBST09UIENBMjAeFw0xNTEwMTkw
     * NzQzMzBaFw0zNTEwMTkwNzQzMzBaMFExCzAJBgNVBAYTAlBMMSgwJgYDVQQKDB9L
     * cmFqb3dhIEl6YmEgUm96bGljemVuaW93YSBTLkEuMRgwFgYDVQQDDA9TWkFGSVIg
     * Uk9PVCBDQTIwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC3vD5QqEvN
     * QLXOYeeWyrSh2gwisPq1e3YAd4wLz32ohswmUeQgPYUM1ljj5/QqGJ3a0a4m7utT
     * 3PSQ1hNKDJA8w/Ta0o4NkjrcsbH/ON7Dui1fgLkCvUqdGw+0w8LBZwPd3BucPbOw
     * 3gAeqDRHu5rr/gsUvTaE2g0gv/pby6kWIK05YO4vdbbnl5z5Pv1+TW9NL++IDWr6
     * 3fE9biCloBK0TXC5ztdyO4mTp4CEHCdJckm1/zuVnsHMyAHs6A6KCpbns6aH5db5
     * BSsNl0BwPLqsdVqc1U2dAgrSS5tmS0YHF2Wtn2yIANwiieDhZNRnvDF5YTy7ykHN
     * XGoAyDw4jlivAgMBAAGjQjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQD
     * AgEGMB0GA1UdDgQWBBQuFqlKGLXLzPVvUPMjX/hd56zwyDANBgkqhkiG9w0BAQsF
     * AAOCAQEAtXP4A9xZWx126aMqe5Aosk3AM0+qmrHUuOQn/6mWmc5G4G18TKI4pAZw
     * 8PRBEew/R40/cof5O/2kbytTAOD/OblqBw7rHRz2onKQy4I9EYKL0rufKq8h5mOG
     * nXkZ7/e7DDWQw4rtTw/1zBLZpD67oPwglV9PJi8RI4NOdQcPv5vRtB3pEAT+ymCP
     * oky4rc/hkA/NrgrHXXu3UNLUYfrVFdvXn4dRVOul4+vJhaAlIDf7js4MNIThPIGy
     * d05DpYhfhmehPea0XGG2Ptv+tyjFogeutcrKjSoS75ftwjCkySp6+/NNIxuZMzSg
     * LvWpCz/UXeHPhJ/iGcJfitYgHuNztw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PL1(0&\x06\x03U\x04\n\x0c\x1fKrajowa Izba Rozliczeniowa S.A.1\x180\x16\x06\x03U\x04\x03\x0c\x0fSZAFIR ROOT CA2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb7\xbc>P\xa8K\xcd@\xb5\xcea\xe7\x96\xca\xb4\xa1\xda\x0c\"\xb0\xfa\xb5{v\x00w\x8c\x0b\xcf}\xa8\x86\xcc&Q\xe4 =\x85\x0c\xd6X\xe3\xe7\xf4*\x18\x9d\xda\xd1\xae&\xee\xebS\xdc\xf4\x90\xd6\x13J\x0c\x90<\xc3\xf4\xda\xd2\x8e\r\x92:\xdc\xb1\xb1\xff8\xde\xc3\xba-_\x80\xb9\x02\xbdJ\x9d\x1b\x0f\xb4\xc3\xc2\xc1g\x03\xdd\xdc\x1b\x9c=\xb3\xb0\xde\x00\x1e\xa84G\xbb\x9a\xeb\xfe\x0b\x14\xbd6\x84\xda\r \xbf\xfa[\xcb\xa9\x16 \xad9`\xee/u\xb6\xe7\x97\x9c\xf9>\xfd~MoM/\xef\x88\rj\xfa\xdd\xf1=n \xa5\xa0\x12\xb4Mp\xb9\xce\xd7r;\x89\x93\xa7\x80\x84\x1c\'IrI\xb5\xff;\x95\x9e\xc1\xcc\xc8\x01\xec\xe8\x0e\x8a\n\x96\xe7\xb3\xa6\x87\xe5\xd6\xf9\x05+\r\x97@p<\xba\xacuZ\x9c\xd5M\x9d\x02\n\xd2K\x9bfKF\x07\x17e\xad\x9fl\x88\x00\xdc\"\x89\xe0\xe1d\xd4g\xbc1ya<\xbb\xcaA\xcd\\j\x00\xc8<8\x8eX\xaf\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=QuoVadis Root Certification Authority O=QuoVadis Limited OU=Root Certification Authority
     * Subject: CN=QuoVadis Root Certification Authority O=QuoVadis Limited OU=Root Certification Authority
     * Label: "QuoVadis Root CA"
     * Serial: 985026699
     * MD5 Fingerprint: 27:de:36:fe:72:b7:00:03:00:9d:f4:f0:1e:6c:04:24
     * SHA1 Fingerprint: de:3f:40:bd:50:93:d3:9b:6c:60:f6:da:bc:07:62:01:00:89:76:c9
     * SHA256 Fingerprint: a4:5e:de:3b:bb:f0:9c:8a:e1:5c:72:ef:c0:72:68:d6:93:a2:1c:99:6f:d5:1e:67:ca:07:94:60:fd:6d:88:73
     * -----BEGIN CERTIFICATE-----
     * MIIF0DCCBLigAwIBAgIEOrZQizANBgkqhkiG9w0BAQUFADB/MQswCQYDVQQGEwJC
     * TTEZMBcGA1UEChMQUXVvVmFkaXMgTGltaXRlZDElMCMGA1UECxMcUm9vdCBDZXJ0
     * aWZpY2F0aW9uIEF1dGhvcml0eTEuMCwGA1UEAxMlUXVvVmFkaXMgUm9vdCBDZXJ0
     * aWZpY2F0aW9uIEF1dGhvcml0eTAeFw0wMTAzMTkxODMzMzNaFw0yMTAzMTcxODMz
     * MzNaMH8xCzAJBgNVBAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBMaW1pdGVkMSUw
     * IwYDVQQLExxSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5MS4wLAYDVQQDEyVR
     * dW9WYWRpcyBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5MIIBIjANBgkqhkiG
     * 9w0BAQEFAAOCAQ8AMIIBCgKCAQEAv2G1lVO6V/z68mcLOhrfEYBklbTRvM16z/Yp
     * li4kVEAkOPcahdxYTMukJ0KX0J+DisPkBgNbAKVRHnAEdOLB1Dqr1607BxgFjv2D
     * rOpm2RgbaIr1VxqYuvXtdj182d6UajtLF8HVj71lODqV0D1VNk7feVcxKh7YWWVJ
     * WCCYfqtffp/p1k3sg3Spx2zY7ilKhSoGFPlU5tPaZQeLYzcS19Dsw3sgQUSj7cug
     * F+FxZc4dZjH3dgEZyH0DWLaVSR2mEiboxgx24ONmy+pdpibu5cxfvWenAScOospU
     * xbF6lR1xHkopigPcakXBpBlebzbNw6Kwt/5cOOJSvPhEQ+aQuwIDAQABo4ICUjCC
     * Ak4wPQYIKwYBBQUHAQEEMTAvMC0GCCsGAQUFBzABhiFodHRwczovL29jc3AucXVv
     * dmFkaXNvZmZzaG9yZS5jb20wDwYDVR0TAQH/BAUwAwEB/zCCARoGA1UdIASCAREw
     * ggENMIIBCQYJKwYBBAG+WAABMIH7MIHUBggrBgEFBQcCAjCBxxqBxFJlbGlhbmNl
     * IG9uIHRoZSBRdW9WYWRpcyBSb290IENlcnRpZmljYXRlIGJ5IGFueSBwYXJ0eSBh
     * c3N1bWVzIGFjY2VwdGFuY2Ugb2YgdGhlIHRoZW4gYXBwbGljYWJsZSBzdGFuZGFy
     * ZCB0ZXJtcyBhbmQgY29uZGl0aW9ucyBvZiB1c2UsIGNlcnRpZmljYXRpb24gcHJh
     * Y3RpY2VzLCBhbmQgdGhlIFF1b1ZhZGlzIENlcnRpZmljYXRlIFBvbGljeS4wIgYI
     * KwYBBQUHAgEWFmh0dHA6Ly93d3cucXVvdmFkaXMuYm0wHQYDVR0OBBYEFItLbe3T
     * KbkGGew5Oanwl4Rqy+/fMIGuBgNVHSMEgaYwgaOAFItLbe3TKbkGGew5Oanwl4Rq
     * y+/foYGEpIGBMH8xCzAJBgNVBAYTAkJNMRkwFwYDVQQKExBRdW9WYWRpcyBMaW1p
     * dGVkMSUwIwYDVQQLExxSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5MS4wLAYD
     * VQQDEyVRdW9WYWRpcyBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5ggQ6tlCL
     * MA4GA1UdDwEB/wQEAwIBBjANBgkqhkiG9w0BAQUFAAOCAQEAitQUtf70mpKnGdSk
     * fnIYj9lofFIk3WdvOXrEql494liwTXCYhGHoG+NpGA7O+0dQoE7/8CQfvbLO9Sf8
     * 7C9TqnN7Az10buYWnuulLsS/VidQK2K6vkscPFVcQR0kvoIgR13VRH56FmjffU1R
     * cHhXHTMe/QKZnAzNCgVPx7uOpHX6Sm2xgI4JVrmcGmD+XcHXetwReNDWXcG31a0y
     * mQM6isxUJTkxgXsTIlG6Rmyhu576BGxJJnSP0nPrzDCi5upZIof4l/UO/erMkqQW
     * xFIY6iHOsfHmhIHluqmGKPJDWl0Snawe2ajlCmqnf6CHKc/yiU3U7MXi5nrQNiOK
     * SnQ2+Q==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BM1\x190\x17\x06\x03U\x04\n\x13\x10QuoVadis Limited1%0#\x06\x03U\x04\x0b\x13\x1cRoot Certification Authority1.0,\x06\x03U\x04\x03\x13%QuoVadis Root Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbfa\xb5\x95S\xbaW\xfc\xfa\xf2g\x0b:\x1a\xdf\x11\x80d\x95\xb4\xd1\xbc\xcdz\xcf\xf6)\x96.$T@$8\xf7\x1a\x85\xdcXL\xcb\xa4\'B\x97\xd0\x9f\x83\x8a\xc3\xe4\x06\x03[\x00\xa5Q\x1ep\x04t\xe2\xc1\xd4:\xab\xd7\xad;\x07\x18\x05\x8e\xfd\x83\xac\xeaf\xd9\x18\x1bh\x8a\xf5W\x1a\x98\xba\xf5\xedv=|\xd9\xde\x94j;K\x17\xc1\xd5\x8f\xbde8:\x95\xd0=U6N\xdfyW1*\x1e\xd8YeIX \x98~\xab_~\x9f\xe9\xd6M\xec\x83t\xa9\xc7l\xd8\xee)J\x85*\x06\x14\xf9T\xe6\xd3\xdae\x07\x8bc7\x12\xd7\xd0\xec\xc3{ AD\xa3\xed\xcb\xa0\x17\xe1qe\xce\x1df1\xf7v\x01\x19\xc8}\x03X\xb6\x95I\x1d\xa6\x12&\xe8\xc6\x0cv\xe0\xe3f\xcb\xea]\xa6&\xee\xe5\xcc_\xbdg\xa7\x01\'\x0e\xa2\xcaT\xc5\xb1z\x95\x1dq\x1eJ)\x8a\x03\xdcjE\xc1\xa4\x19^o6\xcd\xc3\xa2\xb0\xb7\xfe\\8\xe2R\xbc\xf8DC\xe6\x90\xbb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=E-Tugra Certification Authority O=E-Tuğra EBG Bilişim Teknolojileri ve Hizmetleri A.Ş. OU=E-Tugra Sertifikasyon Merkezi
     * Subject: CN=E-Tugra Certification Authority O=E-Tuğra EBG Bilişim Teknolojileri ve Hizmetleri A.Ş. OU=E-Tugra Sertifikasyon Merkezi
     * Label: "E-Tugra Certification Authority"
     * Serial: 7667447206703254355
     * MD5 Fingerprint: b8:a1:03:63:b0:bd:21:71:70:8a:6f:13:3a:bb:79:49
     * SHA1 Fingerprint: 51:c6:e7:08:49:06:6e:f3:92:d4:5c:a0:0d:6d:a3:62:8f:c3:52:39
     * SHA256 Fingerprint: b0:bf:d5:2b:b0:d7:d9:bd:92:bf:5d:4d:c1:3d:a2:55:c0:2c:54:2f:37:83:65:ea:89:39:11:f5:5e:55:f2:3c
     * -----BEGIN CERTIFICATE-----
     * MIIGSzCCBDOgAwIBAgIIamg+nFGby1MwDQYJKoZIhvcNAQELBQAwgbIxCzAJBgNV
     * BAYTAlRSMQ8wDQYDVQQHDAZBbmthcmExQDA+BgNVBAoMN0UtVHXEn3JhIEVCRyBC
     * aWxpxZ9pbSBUZWtub2xvamlsZXJpIHZlIEhpem1ldGxlcmkgQS7Fni4xJjAkBgNV
     * BAsMHUUtVHVncmEgU2VydGlmaWthc3lvbiBNZXJrZXppMSgwJgYDVQQDDB9FLVR1
     * Z3JhIENlcnRpZmljYXRpb24gQXV0aG9yaXR5MB4XDTEzMDMwNTEyMDk0OFoXDTIz
     * MDMwMzEyMDk0OFowgbIxCzAJBgNVBAYTAlRSMQ8wDQYDVQQHDAZBbmthcmExQDA+
     * BgNVBAoMN0UtVHXEn3JhIEVCRyBCaWxpxZ9pbSBUZWtub2xvamlsZXJpIHZlIEhp
     * em1ldGxlcmkgQS7Fni4xJjAkBgNVBAsMHUUtVHVncmEgU2VydGlmaWthc3lvbiBN
     * ZXJrZXppMSgwJgYDVQQDDB9FLVR1Z3JhIENlcnRpZmljYXRpb24gQXV0aG9yaXR5
     * MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA4vU/kwVRHoViVF56C/UY
     * B4Oufq9899SKa6VjQzm5S/fDxmSJPZQuVIBSOTkHS0vdhQd2h8y/L5VMzH2nPbxH
     * D5hw+IyFHnSOkm0bQNGZDbt1bsipa5rAhDGvykPL6ys06I+XawGb1Q5KCKpbknSF
     * Q9OArqGIW66z6l7LFpp3RMih9lRozt6Plyu6W0ACDGQXwLWTzeHxE2bODHnv0ZEo
     * q1+gElIwcxmOj+GMB6LDu0rw6h8VqO4lzKRG+Bsi77MOQ7osJLjFLFzUHPhdZL3D
     * k14opz8n8Y4e0ypQBaNV2cvnOVPAmJ6MVGKLJrD3fY185MaeZkJVgkfnsliNZvcH
     * fC425lAcP9tDJMW/hkd5s3kc91r0E+xs+D/iWR+V7kI+ua2oMoVJl0b+SzGPWsut
     * dEcf6ZG33ygEIqDUD13ieU/qbIWGvaimzuT6w+Gzrt48Ue7LE3wBf4QOXVGUnhMM
     * ti6lTPk5cDZvlsouDERVxcr6XQKj39ZkjFqzAQqptQpHF//vkUAqjqFGOjGY5RH8
     * zLtJVor8udBhmm9lbObDyz51Sf6Pp+KJxWfXnUYTTjF2OySznhFlhqt/7x3U+Lzn
     * rFpct1pHXFXOVbQicVtbC/DP3KBhZOqp12gKY6fgDT+gr9Oq0n7vUaDmUStVkhUX
     * U8u3Zg5mTPj5dUyQ5xJwx0UCAwEAAaNjMGEwHQYDVR0OBBYEFC7j27JJ0JxUeVz6
     * Jyr+zE7S6E5UMA8GA1UdEwEB/wQFMAMBAf8wHwYDVR0jBBgwFoAULuPbsknQnFR5
     * XPonKv7MTtLoTlQwDgYDVR0PAQH/BAQDAgEGMA0GCSqGSIb3DQEBCwUAA4ICAQAF
     * Nzr0TbdF4kV1JI+2d1LoHNgQk2Xz8lkGpD4eKexd0dCrfOAKkEh47U6YA5n+KGCR
     * HTAduGN8qOY1tfrTYXbm1gdLymmasoR6d5NFFxWfJNCYExL/u6Au/U5Mh/jOXKqY
     * GwXgAEZKgoClM4so3O0409/lPun++1ndYYRP0lSWE2ETPo+Aab6TR7U1Q9Jauz1c
     * 77NCR807VRMGsAnb/WP2OogKmW9+4c4bU2pEZiNRCHu8W1Ki/QY3OEBhj0qWuJA3
     * +GbHeJAAFS6LrVE1Uweoa2iu+U48BybNCAVwzDk/dr2l02cmAYamU9JgO3xDf1WK
     * vJUawSg5TB9D0pH0clmKuVb8P7Sd2nCcdlqMQ1DujjByTd//SffGqWfZbawCEeI6
     * FiWnWAjLb1NBnEg4R2gz0dfHj9R0IdTDBZB6/86WiLEVKV0jq9BgoRJP3vQXzTLl
     * yb/IQ639Lo7xr+L0mPoSHyDYwKcMhcWQ9DstliaxLL5Mq+ux0orJ23gTDx4JnW2P
     * AJ8C2sH6H3p6CcRK5ogql5+Ji/03X186zjhZhkuvcQu02PJwT58yE+Owp1fl2tpD
     * y4Q08ijE6m30Ku/Ba3ba+367hTzSU8JNvnHhRdH9I2cNE3X7z2VnIp2usAnRCf8d
     * NL/+I5c30jn6PQ0GC7TbO6Orb1wdtn7os4I07QZcJA==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02TR1\x0f0\r\x06\x03U\x04\x07\x0c\x06Ankara1@0>\x06\x03U\x04\n\x0c7E-Tu\xc4\x9fra EBG Bili\xc5\x9fim Teknolojileri ve Hizmetleri A.\xc5\x9e.1&0$\x06\x03U\x04\x0b\x0c\x1dE-Tugra Sertifikasyon Merkezi1(0&\x06\x03U\x04\x03\x0c\x1fE-Tugra Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xe2\xf5?\x93\x05Q\x1e\x85bT^z\x0b\xf5\x18\x07\x83\xae~\xaf|\xf7\xd4\x8ak\xa5cC9\xb9K\xf7\xc3\xc6d\x89=\x94.T\x80R99\x07KK\xdd\x85\x07v\x87\xcc\xbf/\x95L\xcc}\xa7=\xbcG\x0f\x98p\xf8\x8c\x85\x1et\x8e\x92m\x1b@\xd1\x99\r\xbbun\xc8\xa9k\x9a\xc0\x841\xaf\xcaC\xcb\xeb+4\xe8\x8f\x97k\x01\x9b\xd5\x0eJ\x08\xaa[\x92t\x85C\xd3\x80\xae\xa1\x88[\xae\xb3\xea^\xcb\x16\x9awD\xc8\xa1\xf6Th\xce\xde\x8f\x97+\xba[@\x02\x0cd\x17\xc0\xb5\x93\xcd\xe1\xf1\x13f\xce\x0cy\xef\xd1\x91(\xab_\xa0\x12R0s\x19\x8e\x8f\xe1\x8c\x07\xa2\xc3\xbbJ\xf0\xea\x1f\x15\xa8\xee%\xcc\xa4F\xf8\x1b\"\xef\xb3\x0eC\xba,$\xb8\xc5,\\\xd4\x1c\xf8]d\xbd\xc3\x93^(\xa7?\'\xf1\x8e\x1e\xd3*P\x05\xa3U\xd9\xcb\xe79S\xc0\x98\x9e\x8cTb\x8b&\xb0\xf7}\x8d|\xe4\xc6\x9efBU\x82G\xe7\xb2X\x8df\xf7\x07|.6\xe6P\x1c?\xdbC$\xc5\xbf\x86Gy\xb3y\x1c\xf7Z\xf4\x13\xecl\xf8?\xe2Y\x1f\x95\xeeB>\xb9\xad\xa82\x85I\x97F\xfeK1\x8fZ\xcb\xadtG\x1f\xe9\x91\xb7\xdf(\x04\"\xa0\xd4\x0f]\xe2yO\xeal\x85\x86\xbd\xa8\xa6\xce\xe4\xfa\xc3\xe1\xb3\xae\xde<Q\xee\xcb\x13|\x01\x7f\x84\x0e]Q\x94\x9e\x13\x0c\xb6.\xa5L\xf99p6o\x96\xca.\x0cDU\xc5\xca\xfa]\x02\xa3\xdf\xd6d\x8cZ\xb3\x01\n\xa9\xb5\nG\x17\xff\xef\x91@*\x8e\xa1F:1\x98\xe5\x11\xfc\xcc\xbbIV\x8a\xfc\xb9\xd0a\x9aoel\xe6\xc3\xcb>uI\xfe\x8f\xa7\xe2\x89\xc5g\xd7\x9dF\x13N1v;$\xb3\x9e\x11e\x86\xab\x7f\xef\x1d\xd4\xf8\xbc\xe7\xacZ\\\xb7ZG\\U\xceU\xb4\"q[[\x0b\xf0\xcf\xdc\xa0ad\xea\xa9\xd7h\nc\xa7\xe0\r?\xa0\xaf\xd3\xaa\xd2~\xefQ\xa0\xe6Q+U\x92\x15\x17S\xcb\xb7f\x0efL\xf8\xf9uL\x90\xe7\x12p\xc7E\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Certum Trusted Network CA 2 O=Unizeto Technologies S.A. OU=Certum Certification Authority
     * Subject: CN=Certum Trusted Network CA 2 O=Unizeto Technologies S.A. OU=Certum Certification Authority
     * Label: "Certum Trusted Network CA 2"
     * Serial: 44979900017204383099463764357512596969
     * MD5 Fingerprint: 6d:46:9e:d9:25:6d:08:23:5b:5e:74:7d:1e:27:db:f2
     * SHA1 Fingerprint: d3:dd:48:3e:2b:bf:4c:05:e8:af:10:f5:fa:76:26:cf:d3:dc:30:92
     * SHA256 Fingerprint: b6:76:f2:ed:da:e8:77:5c:d3:6c:b0:f6:3c:d1:d4:60:39:61:f4:9e:62:65:ba:01:3a:2f:03:07:b6:d0:b8:04
     * -----BEGIN CERTIFICATE-----
     * MIIF0jCCA7qgAwIBAgIQIdbQSk8lD8kyN/yqXhKN6TANBgkqhkiG9w0BAQ0FADCB
     * gDELMAkGA1UEBhMCUEwxIjAgBgNVBAoTGVVuaXpldG8gVGVjaG5vbG9naWVzIFMu
     * QS4xJzAlBgNVBAsTHkNlcnR1bSBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTEkMCIG
     * A1UEAxMbQ2VydHVtIFRydXN0ZWQgTmV0d29yayBDQSAyMCIYDzIwMTExMDA2MDgz
     * OTU2WhgPMjA0NjEwMDYwODM5NTZaMIGAMQswCQYDVQQGEwJQTDEiMCAGA1UEChMZ
     * VW5pemV0byBUZWNobm9sb2dpZXMgUy5BLjEnMCUGA1UECxMeQ2VydHVtIENlcnRp
     * ZmljYXRpb24gQXV0aG9yaXR5MSQwIgYDVQQDExtDZXJ0dW0gVHJ1c3RlZCBOZXR3
     * b3JrIENBIDIwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQC9+Xj45tWA
     * DGSdhhuWZGc/IjoedQF97/tcZ4zJzFxrqZHmuULlIEub2pt7uZld2ZuAS9eEQCsn
     * 0+i6MLs+CRqnSZXvK0AkwpfHp+6bJe+oCgCXhVqqndwpyeI1B+twTUrWwbNWuKFB
     * OJvR+zF/j+Bf4bE/D44WSWDXBo0Y+aomEKsq09DRZ40bRr5HMNUuctHFY9rnY3lE
     * fktjJImGLjQ/KUxSiyqnwOKRKIm5wFv5HdnnJ63/mgKXwcZQkpsCLL2puTRZCr+E
     * Sv/f/rOf69me4Jgj7KZrdxYq28ytOxykh9xGc14ZYmhFV+SQgkK7QtbwYeDBoz1m
     * o130GO6IyY0XRSmZMnUCMe4pJshrAua1YkV/NxVaI2iJ1D7eTiew8EAMvE0Xy02i
     * sx7QBlrd9pPPV3WZ9fqGGmd4s7+W/jTcvedSVuWz5XV710GRBdxdaeOVDUO5/IOW
     * OZV7bIBaTxNyxtd9KXpEulKkKtVBRgkg/iKgtlswjbyJDNXXcPiHUv3a76xRLgez
     * Tv7QCdpw75j6VuZt27VXS9zlLCUVyJ4ueE742pyehizKV/Ma5ciSixqClnrDvFAS
     * adgOWkaLOusm+iPJtrCBvkIApPjW/jAux9JG9uWOdf3yzLnQh1vMBhBgu4M1t15n
     * 3kfsmUjxpKEV/q2MYo45VU85FrmxY53/twIDAQABo0IwQDAPBgNVHRMBAf8EBTAD
     * AQH/MB0GA1UdDgQWBBS2oVQ5AsOgP46KvPrU+Bym0ToO/TAOBgNVHQ8BAf8EBAMC
     * AQYwDQYJKoZIhvcNAQENBQADggIBAHGlDs7k6b8/ONWJWsQCYftMxRQXLYtPU2sQ
     * F/xlhMcQSZDe28cmk4gmb3DWAl45oPePq5a1pRNcgRRtDoGCERuKTsZPpd1iHkTf
     * CVn0W3cLN+mLIMb4Ck4uWBzrM9DPhmDJ2vuAL55MYIR4PSFk1vtBHxgP58l1cb29
     * XN40hz5BsA72udY/CROWFC/emh1auVbONTqwX3BNXuMp8SMoclm2q8KMZiYcdywm
     * djWLKKdpoPk79SPdhRB0yZADVpHnr7pH1BKXESLjokmUbOe3lEu6LaTaM4tMpkT/
     * WjzGHWTYtTHkpjx6qFcL2+1hGsvxznN3Y6SHb0xRONbkX8eftoEq5IVIeVheO/jb
     * AoJnwTnbw3RLPTYe+SmTiGhbqEQZIfCn6IENLOiTNrQ3ssqwGyZ6miUfmpqAnksq
     * P/ujmv5zMnHCnsZy4YpoJ/HkD7TETKVhk/iXEAcqMCWpuchxuO9ozC1+9eB+D4Ko
     * b7a6bINDd82Kkhehnlt4Fj1F4jNy3eFmypnTycUm/Q1oBEauttmbjL4ZvrHG8hnj
     * XALKLNhvSgfZyTXaQHXyxKcZb55CEJh15pWLYLztxRLXis7VmFxWlgPF7ncGNf/P
     * 5O4/E2Hu29othfDNrp2yGAlFw5Khchf8R7agCyzxxN5DaAhqXzvwdmP7zAYspsbi
     * DrW5viSP
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PL1\"0 \x06\x03U\x04\n\x13\x19Unizeto Technologies S.A.1\'0%\x06\x03U\x04\x0b\x13\x1eCertum Certification Authority1$0\"\x06\x03U\x04\x03\x13\x1bCertum Trusted Network CA 2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xbd\xf9x\xf8\xe6\xd5\x80\x0cd\x9d\x86\x1b\x96dg?\":\x1eu\x01}\xef\xfb\\g\x8c\xc9\xcc\\k\xa9\x91\xe6\xb9B\xe5 K\x9b\xda\x9b{\xb9\x99]\xd9\x9b\x80K\xd7\x84@+\'\xd3\xe8\xba0\xbb>\t\x1a\xa7I\x95\xef+@$\xc2\x97\xc7\xa7\xee\x9b%\xef\xa8\n\x00\x97\x85Z\xaa\x9d\xdc)\xc9\xe25\x07\xebpMJ\xd6\xc1\xb3V\xb8\xa1A8\x9b\xd1\xfb1\x7f\x8f\xe0_\xe1\xb1?\x0f\x8e\x16I`\xd7\x06\x8d\x18\xf9\xaa&\x10\xab*\xd3\xd0\xd1g\x8d\x1bF\xbeG0\xd5.r\xd1\xc5c\xda\xe7cyD~Kc$\x89\x86.4?)LR\x8b*\xa7\xc0\xe2\x91(\x89\xb9\xc0[\xf9\x1d\xd9\xe7\'\xad\xff\x9a\x02\x97\xc1\xc6P\x92\x9b\x02,\xbd\xa9\xb94Y\n\xbf\x84J\xff\xdf\xfe\xb3\x9f\xeb\xd9\x9e\xe0\x98#\xec\xa6kw\x16*\xdb\xcc\xad;\x1c\xa4\x87\xdcFs^\x19bhEW\xe4\x90\x82B\xbbB\xd6\xf0a\xe0\xc1\xa3=f\xa3]\xf4\x18\xee\x88\xc9\x8d\x17E)\x992u\x021\xee)&\xc8k\x02\xe6\xb5bE\x7f7\x15Z#h\x89\xd4>\xdeN\'\xb0\xf0@\x0c\xbcM\x17\xcbM\xa2\xb3\x1e\xd0\x06Z\xdd\xf6\x93\xcfWu\x99\xf5\xfa\x86\x1agx\xb3\xbf\x96\xfe4\xdc\xbd\xe7RV\xe5\xb3\xe5u{\xd7A\x91\x05\xdc]i\xe3\x95\rC\xb9\xfc\x83\x969\x95{l\x80ZO\x13r\xc6\xd7})zD\xbaR\xa4*\xd5AF\t \xfe\"\xa0\xb6[0\x8d\xbc\x89\x0c\xd5\xd7p\xf8\x87R\xfd\xda\xef\xacQ.\x07\xb3N\xfe\xd0\t\xdap\xef\x98\xfaV\xe6m\xdb\xb5WK\xdc\xe5,%\x15\xc8\x9e.xN\xf8\xda\x9c\x9e\x86,\xcaW\xf3\x1a\xe5\xc8\x92\x8b\x1a\x82\x96z\xc3\xbcP\x12i\xd8\x0eZF\x8b:\xeb&\xfa#\xc9\xb6\xb0\x81\xbeB\x00\xa4\xf8\xd6\xfe0.\xc7\xd2F\xf6\xe5\x8eu\xfd\xf2\xcc\xb9\xd0\x87[\xcc\x06\x10`\xbb\x835\xb7^g\xdeG\xec\x99H\xf1\xa4\xa1\x15\xfe\xad\x8cb\x8e9UO9\x16\xb9\xb1c\x9d\xff\xb7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Hellenic Academic and Research Institutions RootCA 2011 O=Hellenic Academic and Research Institutions Cert. Authority
     * Subject: CN=Hellenic Academic and Research Institutions RootCA 2011 O=Hellenic Academic and Research Institutions Cert. Authority
     * Label: "Hellenic Academic and Research Institutions RootCA 2011"
     * Serial: 0
     * MD5 Fingerprint: 73:9f:4c:4b:73:5b:79:e9:fa:ba:1c:ef:6e:cb:d5:c9
     * SHA1 Fingerprint: fe:45:65:9b:79:03:5b:98:a1:61:b5:51:2e:ac:da:58:09:48:22:4d
     * SHA256 Fingerprint: bc:10:4f:15:a4:8b:e7:09:dc:a5:42:a7:e1:d4:b9:df:6f:05:45:27:e8:02:ea:a9:2d:59:54:44:25:8a:fe:71
     * -----BEGIN CERTIFICATE-----
     * MIIEMTCCAxmgAwIBAgIBADANBgkqhkiG9w0BAQUFADCBlTELMAkGA1UEBhMCR1Ix
     * RDBCBgNVBAoTO0hlbGxlbmljIEFjYWRlbWljIGFuZCBSZXNlYXJjaCBJbnN0aXR1
     * dGlvbnMgQ2VydC4gQXV0aG9yaXR5MUAwPgYDVQQDEzdIZWxsZW5pYyBBY2FkZW1p
     * YyBhbmQgUmVzZWFyY2ggSW5zdGl0dXRpb25zIFJvb3RDQSAyMDExMB4XDTExMTIw
     * NjEzNDk1MloXDTMxMTIwMTEzNDk1MlowgZUxCzAJBgNVBAYTAkdSMUQwQgYDVQQK
     * EztIZWxsZW5pYyBBY2FkZW1pYyBhbmQgUmVzZWFyY2ggSW5zdGl0dXRpb25zIENl
     * cnQuIEF1dGhvcml0eTFAMD4GA1UEAxM3SGVsbGVuaWMgQWNhZGVtaWMgYW5kIFJl
     * c2VhcmNoIEluc3RpdHV0aW9ucyBSb290Q0EgMjAxMTCCASIwDQYJKoZIhvcNAQEB
     * BQADggEPADCCAQoCggEBAKlTAOMupvaO+mDYLZU++CwqVE7NuYRhlFhPjz2L5EPz
     * dYmNUeTDN9KKiE15HrcS3UN4SoqS5tdI1Q+kOilENbgH9mgdVc04UfCMJDGFr4PJ
     * fel3r+0ae50X+bOdOFAPplp5kYCvN66m0zH7tSYJnTxa71HFK9+WXesyHgLacEns
     * bgzImjeN9/E2YEsmLIKe0HjzDQ9jpFEw4fkrJxIH2Oq9GGKYsFk3fb7u8yBRQlqD
     * 75O6aRXxYp2fmTmCobd0LovUxQt7L/DICto9eQqakxylKHJzkUOap9FNhYS5qXSP
     * FEDH3N6sQWRstBmbAmNtJGSPRLIl6s5ddAxjMlyNh+UCAwEAAaOBiTCBhjAPBgNV
     * HRMBAf8EBTADAQH/MAsGA1UdDwQEAwIBBjAdBgNVHQ4EFgQUppFC/RNhSiOeCKQp
     * 5dgTBCPuQSUwRwYDVR0eBEAwPqA8MAWCAy5ncjAFggMuZXUwBoIELmVkdTAGggQu
     * b3JnMAWBAy5ncjAFgQMuZXUwBoEELmVkdTAGgQQub3JnMA0GCSqGSIb3DQEBBQUA
     * A4IBAQAf73lB4XtuP7KMhjdCSk4cNx6NZrokgclPEg8hwAOXhiVtXdMiKahsog2p
     * 6z0GW5k6x8zDmjR/qw7IThzh+uTczQ2+vyT+bOdrwg3IBp5OjWEopmr95fZi6hg8
     * TqBTnbI6nOulnJEWtk2C4AwFSKls9cz4y51JtPACpf1wA+2KIaWuE4ZJwzNzvoc7
     * dIsXRSZMFpGD/md9zU1jZ/rzAxKWeAaNsWftjj++n08C9bMJL/NMh98qy5V8Acys
     * Nnq/onN694/BtZqhFLKPM58N7yLcZnuEvUUXBj08yrl3NI/K6s8/MT7jiOOASSXI
     * l7WdmplNsDz4SgCbZN2fOUvRJ9e4
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GR1D0B\x06\x03U\x04\n\x13;Hellenic Academic and Research Institutions Cert. Authority1@0>\x06\x03U\x04\x03\x137Hellenic Academic and Research Institutions RootCA 2011",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xa9S\x00\xe3.\xa6\xf6\x8e\xfa`\xd8-\x95>\xf8,*TN\xcd\xb9\x84a\x94XO\x8f=\x8b\xe4C\xf3u\x89\x8dQ\xe4\xc37\xd2\x8a\x88My\x1e\xb7\x12\xddCxJ\x8a\x92\xe6\xd7H\xd5\x0f\xa4:)D5\xb8\x07\xf6h\x1dU\xcd8Q\xf0\x8c$1\x85\xaf\x83\xc9}\xe9w\xaf\xed\x1a{\x9d\x17\xf9\xb3\x9d8P\x0f\xa6Zy\x91\x80\xaf7\xae\xa6\xd31\xfb\xb5&\t\x9d<Z\xefQ\xc5+\xdf\x96]\xeb2\x1e\x02\xdapI\xecn\x0c\xc8\x9a7\x8d\xf7\xf16`K&,\x82\x9e\xd0x\xf3\r\x0fc\xa4Q0\xe1\xf9+\'\x12\x07\xd8\xea\xbd\x18b\x98\xb0Y7}\xbe\xee\xf3 QBZ\x83\xef\x93\xbai\x15\xf1b\x9d\x9f\x999\x82\xa1\xb7t.\x8b\xd4\xc5\x0b{/\xf0\xc8\n\xda=y\n\x9a\x93\x1c\xa5(rs\x91C\x9a\xa7\xd1M\x85\x84\xb9\xa9t\x8f\x14@\xc7\xdc\xde\xacAdl\xb4\x19\x9b\x02cm$d\x8fD\xb2%\xea\xce]t\x0cc2\\\x8d\x87\xe5\x02\x03\x01\x00\x01",
        name_constraints: Some(b"\xa0<0\x05\x82\x03.gr0\x05\x82\x03.eu0\x06\x82\x04.edu0\x06\x82\x04.org0\x05\x81\x03.gr0\x05\x81\x03.eu0\x06\x81\x04.edu0\x06\x81\x04.org")
    },

    /*
     * Issuer: CN=emSign ECC Root CA - C3 O=eMudhra Inc OU=emSign PKI
     * Subject: CN=emSign ECC Root CA - C3 O=eMudhra Inc OU=emSign PKI
     * Label: "emSign ECC Root CA - C3"
     * Serial: 582948710642506000014504
     * MD5 Fingerprint: 3e:53:b3:a3:81:ee:d7:10:f8:d3:b0:1d:17:92:f5:d5
     * SHA1 Fingerprint: b6:af:43:c2:9b:81:53:7d:f6:ef:6b:c3:1f:1f:60:15:0c:ee:48:66
     * SHA256 Fingerprint: bc:4d:80:9b:15:18:9d:78:db:3e:1d:8c:f4:f9:72:6a:79:5d:a1:64:3c:a5:f1:35:8e:1d:db:0e:dc:0d:7e:b3
     * -----BEGIN CERTIFICATE-----
     * MIICKzCCAbGgAwIBAgIKe3G2gla4EnycqDAKBggqhkjOPQQDAzBaMQswCQYDVQQG
     * EwJVUzETMBEGA1UECxMKZW1TaWduIFBLSTEUMBIGA1UEChMLZU11ZGhyYSBJbmMx
     * IDAeBgNVBAMTF2VtU2lnbiBFQ0MgUm9vdCBDQSAtIEMzMB4XDTE4MDIxODE4MzAw
     * MFoXDTQzMDIxODE4MzAwMFowWjELMAkGA1UEBhMCVVMxEzARBgNVBAsTCmVtU2ln
     * biBQS0kxFDASBgNVBAoTC2VNdWRocmEgSW5jMSAwHgYDVQQDExdlbVNpZ24gRUND
     * IFJvb3QgQ0EgLSBDMzB2MBAGByqGSM49AgEGBSuBBAAiA2IABP2lYa57JhAd6bci
     * MK4G9IGzsUJxlTm801Ljr6/58pc1kjZGDoeVjbk5Wum739D+yAdBPLtVb4Ojavti
     * sIGJAnB9SMVK4+kiVCJNk7tCDK93nCOmfddhEc5lx/h//vXyqaNCMEAwHQYDVR0O
     * BBYEFPtaSNCAIEDyqOkAB2kZd6fmw/TPMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMB
     * Af8EBTADAQH/MAoGCCqGSM49BAMDA2gAMGUCMQC02C8Cif22TGK6Q04ThHK1rt0c
     * 3ta13FaPWEBaLd4gTCKDypOofu4SQMfWh0/434UCMBwUZOR8loMRnLDRWmFLpg9J
     * 0wD8ofzkpf9/rdcw0Md3f76BB1UwUCAU9Vc4CqgxUQ==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x130\x11\x06\x03U\x04\x0b\x13\nemSign PKI1\x140\x12\x06\x03U\x04\n\x13\x0beMudhra Inc1 0\x1e\x06\x03U\x04\x03\x13\x17emSign ECC Root CA - C3",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xfd\xa5a\xae{&\x10\x1d\xe9\xb7\"0\xae\x06\xf4\x81\xb3\xb1Bq\x959\xbc\xd3R\xe3\xaf\xaf\xf9\xf2\x975\x926F\x0e\x87\x95\x8d\xb99Z\xe9\xbb\xdf\xd0\xfe\xc8\x07A<\xbbUo\x83\xa3j\xfbb\xb0\x81\x89\x02p}H\xc5J\xe3\xe9\"T\"M\x93\xbbB\x0c\xafw\x9c#\xa6}\xd7a\x11\xcee\xc7\xf8\x7f\xfe\xf5\xf2\xa9",
        name_constraints: None
    },

    /*
     * Issuer: CN=AffirmTrust Premium ECC O=AffirmTrust
     * Subject: CN=AffirmTrust Premium ECC O=AffirmTrust
     * Label: "AffirmTrust Premium ECC"
     * Serial: 8401224907861490260
     * MD5 Fingerprint: 64:b0:09:55:cf:b1:d5:99:e2:be:13:ab:a6:5d:ea:4d
     * SHA1 Fingerprint: b8:23:6b:00:2f:1d:16:86:53:01:55:6c:11:a4:37:ca:eb:ff:c3:bb
     * SHA256 Fingerprint: bd:71:fd:f6:da:97:e4:cf:62:d1:64:7a:dd:25:81:b0:7d:79:ad:f8:39:7e:b4:ec:ba:9c:5e:84:88:82:14:23
     * -----BEGIN CERTIFICATE-----
     * MIIB/jCCAYWgAwIBAgIIdJclisc/elQwCgYIKoZIzj0EAwMwRTELMAkGA1UEBhMC
     * VVMxFDASBgNVBAoMC0FmZmlybVRydXN0MSAwHgYDVQQDDBdBZmZpcm1UcnVzdCBQ
     * cmVtaXVtIEVDQzAeFw0xMDAxMjkxNDIwMjRaFw00MDEyMzExNDIwMjRaMEUxCzAJ
     * BgNVBAYTAlVTMRQwEgYDVQQKDAtBZmZpcm1UcnVzdDEgMB4GA1UEAwwXQWZmaXJt
     * VHJ1c3QgUHJlbWl1bSBFQ0MwdjAQBgcqhkjOPQIBBgUrgQQAIgNiAAQNMF4bFZ0D
     * 0KF5Nbc6PJJ6yhUczWLznCZcBz3lVPqj1swS6vQUX+iOGasvLkjmrBhDeKzQN8O9
     * ss0s5kfiGuZjuD0uL3jET9v0D6RoTFVya5UdThhClXjMNzyR4ptlKymjQjBAMB0G
     * A1UdDgQWBBSaryl6wBE1NSZRMADDav5A1a7WPDAPBgNVHRMBAf8EBTADAQH/MA4G
     * A1UdDwEB/wQEAwIBBjAKBggqhkjOPQQDAwNnADBkAjAXCfOHiFBar8jAQr9HX/Vs
     * aobgxCd05DhT1wV/GzTjxi+zygk8N53X57hG8f2h4nECMEJZh0PUUd+60wkyWs6I
     * flc9nF9Ca/UHLbXwgpP5WW+uZPpY5Yse42O+tYHNbwKMeQ==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x140\x12\x06\x03U\x04\n\x0c\x0bAffirmTrust1 0\x1e\x06\x03U\x04\x03\x0c\x17AffirmTrust Premium ECC",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\r0^\x1b\x15\x9d\x03\xd0\xa1y5\xb7:<\x92z\xca\x15\x1c\xcdb\xf3\x9c&\\\x07=\xe5T\xfa\xa3\xd6\xcc\x12\xea\xf4\x14_\xe8\x8e\x19\xab/.H\xe6\xac\x18Cx\xac\xd07\xc3\xbd\xb2\xcd,\xe6G\xe2\x1a\xe6c\xb8=./x\xc4O\xdb\xf4\x0f\xa4hLUrk\x95\x1dN\x18B\x95x\xcc7<\x91\xe2\x9be+)",
        name_constraints: None
    },

    /*
     * Issuer: CN=SwissSign Silver CA - G2 O=SwissSign AG
     * Subject: CN=SwissSign Silver CA - G2 O=SwissSign AG
     * Label: "SwissSign Silver CA - G2"
     * Serial: 5700383053117599563
     * MD5 Fingerprint: e0:06:a1:c9:7d:cf:c9:fc:0d:c0:56:75:96:d8:62:13
     * SHA1 Fingerprint: 9b:aa:e5:9f:56:ee:21:cb:43:5a:be:25:93:df:a7:f0:40:d1:1d:cb
     * SHA256 Fingerprint: be:6c:4d:a2:bb:b9:ba:59:b6:f3:93:97:68:37:42:46:c3:c0:05:99:3f:a9:8f:02:0d:1d:ed:be:d4:8a:81:d5
     * -----BEGIN CERTIFICATE-----
     * MIIFvTCCA6WgAwIBAgIITxvUL1S7L0swDQYJKoZIhvcNAQEFBQAwRzELMAkGA1UE
     * BhMCQ0gxFTATBgNVBAoTDFN3aXNzU2lnbiBBRzEhMB8GA1UEAxMYU3dpc3NTaWdu
     * IFNpbHZlciBDQSAtIEcyMB4XDTA2MTAyNTA4MzI0NloXDTM2MTAyNTA4MzI0Nlow
     * RzELMAkGA1UEBhMCQ0gxFTATBgNVBAoTDFN3aXNzU2lnbiBBRzEhMB8GA1UEAxMY
     * U3dpc3NTaWduIFNpbHZlciBDQSAtIEcyMIICIjANBgkqhkiG9w0BAQEFAAOCAg8A
     * MIICCgKCAgEAxPGHf9N4Mfc4yfjDmUO8x/e8N+dOcbpLj6VzHVxumK4DV644N0Mv
     * Fz0fyM5oEMF4rhkDKxD6LHmD9ui5aLlV8gREpzn5/ASLHvGiTSf5YXu6t+WiE7br
     * YT7QbNHm+/pe7R20nqA1W6GSy/BJkv6FCgU+5tkL4k+73JU3/JHpMjUi0R86TieF
     * nbAVlDLaYQ1HTWBCrpJH6INaUFjpiou5XaHc3ZlKHzZnu0jkg7Y360g6rw9njxcH
     * 6ATK72oxh9TAtvmUcXtnZLi2kUpCe2UuMGoM9ZDulebyzYLs2aFK7PayS+VFheZt
     * eJMELpyCbTapxDFkH4aDCyr0NQp4yVXPQbBH6TCfmb5hqAaEuSh6XzjZG6k4sIN/
     * c8HDO0gqgg8hm7jMqDXDhBuDsz6+pJVpATqJAHgE2cn0mRmrVn5bi4Y5FZGkECwJ
     * MoBgs5PAKrYYC51+jUnyEEp/+dVGLxmSo5mnJqy7jDzmDrxHB9xzUfFwZC8I+bRH
     * HTBsROopN4WSaGa8gzj+ezku01DwH/teYLappvonQfGbGHLy9YR0SslnxFSuSGTf
     * jNFusB3hB48IHpmccelM2KX3RxIfdNFRnobzwqIjQAtz20um53MGjMGg6cFZrEb6
     * 5i/4z3GcRm25xBWNOHkDRUjvxF3XCO6HOSKGsg0PWEP3calILv3q1h8CAwEAAaOB
     * rDCBqTAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQU
     * F6DNweRBtjpbO8tFnb0cwpj6hlgwHwYDVR0jBBgwFoAUF6DNweRBtjpbO8tFnb0c
     * wpj6hlgwRgYDVR0gBD8wPTA7BglghXQBWQEDAQEwLjAsBggrBgEFBQcCARYgaHR0
     * cDovL3JlcG9zaXRvcnkuc3dpc3NzaWduLmNvbS8wDQYJKoZIhvcNAQEFBQADggIB
     * AHPGgeAn0i0P4JUw4ppBf1AsX19iYamGamkYDHRJ1l2E6kFSGG9YrVBWIGrGvShp
     * WJHckRE1qTodvBqlYJ7YH39FkWnZfrt4csEGDyrOj4VwYaygzQu4OSlWhDJOhrs9
     * xCrZ1x9y7v5RoSJBsXECYxqCsGKrXlcSH9/L3XWgwF15kIwb4FDm3jH+mHtwX6WQ
     * 2K34ArZv02DdQEsixT2tOnqfGhpHkXkzuoLcMmkDlm4fS/Bx/uNncqCxv1yL5PqZ
     * IseEuRuNI5c/7SXgz2W79WEE790eslpBIlqhn10s6FvJbakMDHiqYMZWjwFaDGi8
     * aRl5xB9+lwW/xekkUV7U1UtT7dkjWjYDZaPBA61BMPNGG4WQr2W11bHkFlt4dR2X
     * em1ZqSqPe97Dh4kQmUlzeMg9vVE1dCrV8X5pGyq7O70luJpaPXJhkGaH7gzWTdQR
     * dAtq/gsD/KNVV4n+SsuuWxcFyPKNIzFTONItaj+CuY0IavdeQXRuwxF+B6wpYJE/
     * OMpXEA29MC/HpeZBoNquBYeaoKRlbEwJDIm6uNO5wJOKMPqN5ZprFQFOZ6raYlY+
     * hAhm0sQ2fac+EPyI4NSA5QC9qvNOBqN6avlicuMJT+ubDgEj8Z+7fNzcbBGXJbLy
     * tGMU0gYqZ4yD9c7qB9iaah7s5Aq7KkzrCWA5zspi2C5u
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CH1\x150\x13\x06\x03U\x04\n\x13\x0cSwissSign AG1!0\x1f\x06\x03U\x04\x03\x13\x18SwissSign Silver CA - G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc4\xf1\x87\x7f\xd3x1\xf78\xc9\xf8\xc3\x99C\xbc\xc7\xf7\xbc7\xe7Nq\xbaK\x8f\xa5s\x1d\\n\x98\xae\x03W\xae87C/\x17=\x1f\xc8\xceh\x10\xc1x\xae\x19\x03+\x10\xfa,y\x83\xf6\xe8\xb9h\xb9U\xf2\x04D\xa79\xf9\xfc\x04\x8b\x1e\xf1\xa2M\'\xf9a{\xba\xb7\xe5\xa2\x13\xb6\xeba>\xd0l\xd1\xe6\xfb\xfa^\xed\x1d\xb4\x9e\xa05[\xa1\x92\xcb\xf0I\x92\xfe\x85\n\x05>\xe6\xd9\x0b\xe2O\xbb\xdc\x957\xfc\x91\xe925\"\xd1\x1f:N\'\x85\x9d\xb0\x15\x942\xdaa\rGM`B\xae\x92G\xe8\x83ZPX\xe9\x8a\x8b\xb9]\xa1\xdc\xdd\x99J\x1f6g\xbbH\xe4\x83\xb67\xebH:\xaf\x0fg\x8f\x17\x07\xe8\x04\xca\xefj1\x87\xd4\xc0\xb6\xf9\x94q{gd\xb8\xb6\x91JB{e.0j\x0c\xf5\x90\xee\x95\xe6\xf2\xcd\x82\xec\xd9\xa1J\xec\xf6\xb2K\xe5E\x85\xe6mx\x93\x04.\x9c\x82m6\xa9\xc41d\x1f\x86\x83\x0b*\xf45\nx\xc9U\xcfA\xb0G\xe90\x9f\x99\xbea\xa8\x06\x84\xb9(z_8\xd9\x1b\xa98\xb0\x83\x7fs\xc1\xc3;H*\x82\x0f!\x9b\xb8\xcc\xa85\xc3\x84\x1b\x83\xb3>\xbe\xa4\x95i\x01:\x89\x00x\x04\xd9\xc9\xf4\x99\x19\xabV~[\x8b\x869\x15\x91\xa4\x10,\t2\x80`\xb3\x93\xc0*\xb6\x18\x0b\x9d~\x8dI\xf2\x10J\x7f\xf9\xd5F/\x19\x92\xa3\x99\xa7&\xac\xbb\x8c<\xe6\x0e\xbcG\x07\xdcsQ\xf1pd/\x08\xf9\xb4G\x1d0lD\xea)7\x85\x92hf\xbc\x838\xfe{9.\xd3P\xf0\x1f\xfb^`\xb6\xa9\xa6\xfa\'A\xf1\x9b\x18r\xf2\xf5\x84tJ\xc9g\xc4T\xaeHd\xdf\x8c\xd1n\xb0\x1d\xe1\x07\x8f\x08\x1e\x99\x9cq\xe9L\xd8\xa5\xf7G\x12\x1ft\xd1Q\x9e\x86\xf3\xc2\xa2#@\x0bs\xdbK\xa6\xe7s\x06\x8c\xc1\xa0\xe9\xc1Y\xacF\xfa\xe6/\xf8\xcfq\x9cFm\xb9\xc4\x15\x8d8y\x03EH\xef\xc4]\xd7\x08\xee\x879\"\x86\xb2\r\x0fXC\xf7q\xa9H.\xfd\xea\xd6\x1f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=e-Szigno Root CA 2017 O=Microsec Ltd.
     * Subject: CN=e-Szigno Root CA 2017 O=Microsec Ltd.
     * Label: "e-Szigno Root CA 2017"
     * Serial: 411379200276854331539784714
     * MD5 Fingerprint: de:1f:f6:9e:84:ae:a7:b4:21:ce:1e:58:7d:d1:84:98
     * SHA1 Fingerprint: 89:d4:83:03:4f:9e:9a:48:80:5f:72:37:d4:a9:a6:ef:cb:7c:1f:d1
     * SHA256 Fingerprint: be:b0:0b:30:83:9b:9b:c3:2c:32:e4:44:79:05:95:06:41:f2:64:21:b1:5e:d0:89:19:8b:51:8a:e2:ea:1b:99
     * -----BEGIN CERTIFICATE-----
     * MIICQDCCAeWgAwIBAgIMAVRI7yH9l1kN9QQKMAoGCCqGSM49BAMCMHExCzAJBgNV
     * BAYTAkhVMREwDwYDVQQHDAhCdWRhcGVzdDEWMBQGA1UECgwNTWljcm9zZWMgTHRk
     * LjEXMBUGA1UEYQwOVkFUSFUtMjM1ODQ0OTcxHjAcBgNVBAMMFWUtU3ppZ25vIFJv
     * b3QgQ0EgMjAxNzAeFw0xNzA4MjIxMjA3MDZaFw00MjA4MjIxMjA3MDZaMHExCzAJ
     * BgNVBAYTAkhVMREwDwYDVQQHDAhCdWRhcGVzdDEWMBQGA1UECgwNTWljcm9zZWMg
     * THRkLjEXMBUGA1UEYQwOVkFUSFUtMjM1ODQ0OTcxHjAcBgNVBAMMFWUtU3ppZ25v
     * IFJvb3QgQ0EgMjAxNzBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABJbcPYrYsHtv
     * xie+RJCxs1YVe45DJH0ahFnuY2iyxl6H0BVIHqiQrb1TotreOpCmYF9oMrWGQd+H
     * Wyx7xf58etqjYzBhMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMB0G
     * A1UdDgQWBBSHERUI0arBeAyxr87GyZDvvzAEwDAfBgNVHSMEGDAWgBSHERUI0arB
     * eAyxr87GyZDvvzAEwDAKBggqhkjOPQQDAgNJADBGAiEAtVfd14pVCzbhhkT61Nlo
     * jbjcI4qKDdQvfepz7L9NbKgCIQDLpbQS+ue16M9+k/zzNY9vTlp8tLxOsvxyqltZ
     * +efcMQ==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02HU1\x110\x0f\x06\x03U\x04\x07\x0c\x08Budapest1\x160\x14\x06\x03U\x04\n\x0c\rMicrosec Ltd.1\x170\x15\x06\x03U\x04a\x0c\x0eVATHU-235844971\x1e0\x1c\x06\x03U\x04\x03\x0c\x15e-Szigno Root CA 2017",
        spki: b"0\x13\x06\x07*\x86H\xce=\x02\x01\x06\x08*\x86H\xce=\x03\x01\x07\x03B\x00\x04\x96\xdc=\x8a\xd8\xb0{o\xc6\'\xbeD\x90\xb1\xb3V\x15{\x8eC$}\x1a\x84Y\xeech\xb2\xc6^\x87\xd0\x15H\x1e\xa8\x90\xad\xbdS\xa2\xda\xde:\x90\xa6`_h2\xb5\x86A\xdf\x87[,{\xc5\xfe|z\xda",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign O=GlobalSign OU=GlobalSign ECC Root CA - R4
     * Subject: CN=GlobalSign O=GlobalSign OU=GlobalSign ECC Root CA - R4
     * Label: "GlobalSign ECC Root CA - R4"
     * Serial: 14367148294922964480859022125800977897474
     * MD5 Fingerprint: 20:f0:27:68:d1:7e:a0:9d:0e:e6:2a:ca:df:5c:89:8e
     * SHA1 Fingerprint: 69:69:56:2e:40:80:f4:24:a1:e7:19:9f:14:ba:f3:ee:58:ab:6a:bb
     * SHA256 Fingerprint: be:c9:49:11:c2:95:56:76:db:6c:0a:55:09:86:d7:6e:3b:a0:05:66:7c:44:2c:97:62:b4:fb:b7:73:de:22:8c
     * -----BEGIN CERTIFICATE-----
     * MIIB4TCCAYegAwIBAgIRKjikHJYKBN5CsiilC+g0mAIwCgYIKoZIzj0EAwIwUDEk
     * MCIGA1UECxMbR2xvYmFsU2lnbiBFQ0MgUm9vdCBDQSAtIFI0MRMwEQYDVQQKEwpH
     * bG9iYWxTaWduMRMwEQYDVQQDEwpHbG9iYWxTaWduMB4XDTEyMTExMzAwMDAwMFoX
     * DTM4MDExOTAzMTQwN1owUDEkMCIGA1UECxMbR2xvYmFsU2lnbiBFQ0MgUm9vdCBD
     * QSAtIFI0MRMwEQYDVQQKEwpHbG9iYWxTaWduMRMwEQYDVQQDEwpHbG9iYWxTaWdu
     * MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEuMZ5049sJQ6fLjkZHAOkrprlOQcJ
     * FspjsbmG+IpXwVfOQvpzofdlQv8ewQCybnMO/8ch5RikqtlxP6jUuc6MHaNCMEAw
     * DgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFFSwe61F
     * uOJAf/sKbvu+M8k8o4TVMAoGCCqGSM49BAMCA0gAMEUCIQDckqGgE6bPA7DmxCGX
     * kPoUVy0D7O48027KqGx2vKLeuwIgJ6iFJzWbVsaj8kfSt24bAgAXqmemFZHe+pTs
     * ewv4n4Q=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1$0\"\x06\x03U\x04\x0b\x13\x1bGlobalSign ECC Root CA - R41\x130\x11\x06\x03U\x04\n\x13\nGlobalSign1\x130\x11\x06\x03U\x04\x03\x13\nGlobalSign",
        spki: b"0\x13\x06\x07*\x86H\xce=\x02\x01\x06\x08*\x86H\xce=\x03\x01\x07\x03B\x00\x04\xb8\xc6y\xd3\x8fl%\x0e\x9f.9\x19\x1c\x03\xa4\xae\x9a\xe59\x07\t\x16\xcac\xb1\xb9\x86\xf8\x8aW\xc1W\xceB\xfas\xa1\xf7eB\xff\x1e\xc1\x00\xb2ns\x0e\xff\xc7!\xe5\x18\xa4\xaa\xd9q?\xa8\xd4\xb9\xce\x8c\x1d",
        name_constraints: None
    },

    /*
     * Issuer: CN=SecureSign RootCA11 O=Japan Certification Services, Inc.
     * Subject: CN=SecureSign RootCA11 O=Japan Certification Services, Inc.
     * Label: "SecureSign RootCA11"
     * Serial: 1
     * MD5 Fingerprint: b7:52:74:e2:92:b4:80:93:f2:75:e4:cc:d7:f2:ea:26
     * SHA1 Fingerprint: 3b:c4:9f:48:f8:f3:73:a0:9c:1e:bd:f8:5b:b1:c3:65:c7:d8:11:b3
     * SHA256 Fingerprint: bf:0f:ee:fb:9e:3a:58:1a:d5:f9:e9:db:75:89:98:57:43:d2:61:08:5c:4d:31:4f:6f:5d:72:59:aa:42:16:12
     * -----BEGIN CERTIFICATE-----
     * MIIDbTCCAlWgAwIBAgIBATANBgkqhkiG9w0BAQUFADBYMQswCQYDVQQGEwJKUDEr
     * MCkGA1UEChMiSmFwYW4gQ2VydGlmaWNhdGlvbiBTZXJ2aWNlcywgSW5jLjEcMBoG
     * A1UEAxMTU2VjdXJlU2lnbiBSb290Q0ExMTAeFw0wOTA0MDgwNDU2NDdaFw0yOTA0
     * MDgwNDU2NDdaMFgxCzAJBgNVBAYTAkpQMSswKQYDVQQKEyJKYXBhbiBDZXJ0aWZp
     * Y2F0aW9uIFNlcnZpY2VzLCBJbmMuMRwwGgYDVQQDExNTZWN1cmVTaWduIFJvb3RD
     * QTExMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA/XeqpRyQBTvLTJsz
     * i1oURaTnkBbR31fSIRCkF/3frNYfp+TbfPfs37gD2pRY/V1yfIw/XwFndBWW4wI8
     * h9uuywGOwvNmxoVF9ALGOrVisq/6nL+k5tSAMJjzDbaTj6nU2DbysPyKyiyhFTOV
     * MdrAG/LuYpmGYz+/3ZMqg6h2uRMft85OQoWPIucuGvKVCbIFtUROd6EgvanyTgp9
     * UK31BQ1FT0Zx/Sg+U/sE2C3XZR1KG/rPO7AxmjVuyIsG0wCR8pQIZUyxNAYAeoni
     * 8McDWc/V1uinMrPmmECGxc0nEovMe863ETxiYAcjPitAbpSACW22s293bzUIUPsC
     * h8U+iQIDAQABo0IwQDAdBgNVHQ4EFgQUW/hNT7KlhtQ60vFjmqC+CfZXt94wDgYD
     * VR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQEFBQADggEB
     * AKChOBZmLqdWHyGcBvod7bkixTgm2E5P7KN/ed5GIaGHd48HCJqypMWvDzKYC3xm
     * KbabfSVSSUOrTC4rbnpwrxYO4wJs+0LmGJ1F2FXI6Dvd5+H0LgscNFxsWEr7jIhQ
     * X5Ucv+2rIrVls4W6ng+4reV6G4pQOh29Dbx7VFALuUKvVaAYga1lme++5Jy/xIWr
     * QbJUb9wlze144o4MjQlJ3WN7WmmWAiGovVJZ6X01y8hSyn+B/tlr0/cR7SXf+Of5
     * pPpyl4RTDaXQMhhRdlkUbA/r7F+AjHVDg8OFmP9Mni0N5HeDk061lgeLKBObjBmN
     * QSdJQO7e5iNEOdyhIta6A/I=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02JP1+0)\x06\x03U\x04\n\x13\"Japan Certification Services, Inc.1\x1c0\x1a\x06\x03U\x04\x03\x13\x13SecureSign RootCA11",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xfdw\xaa\xa5\x1c\x90\x05;\xcbL\x9b3\x8bZ\x14E\xa4\xe7\x90\x16\xd1\xdfW\xd2!\x10\xa4\x17\xfd\xdf\xac\xd6\x1f\xa7\xe4\xdb|\xf7\xec\xdf\xb8\x03\xda\x94X\xfd]r|\x8c?_\x01gt\x15\x96\xe3\x02<\x87\xdb\xae\xcb\x01\x8e\xc2\xf3f\xc6\x85E\xf4\x02\xc6:\xb5b\xb2\xaf\xfa\x9c\xbf\xa4\xe6\xd4\x800\x98\xf3\r\xb6\x93\x8f\xa9\xd4\xd86\xf2\xb0\xfc\x8a\xca,\xa1\x153\x951\xda\xc0\x1b\xf2\xeeb\x99\x86c?\xbf\xdd\x93*\x83\xa8v\xb9\x13\x1f\xb7\xceNB\x85\x8f\"\xe7.\x1a\xf2\x95\t\xb2\x05\xb5DNw\xa1 \xbd\xa9\xf2N\n}P\xad\xf5\x05\rEOFq\xfd(>S\xfb\x04\xd8-\xd7e\x1dJ\x1b\xfa\xcf;\xb01\x9a5n\xc8\x8b\x06\xd3\x00\x91\xf2\x94\x08eL\xb14\x06\x00z\x89\xe2\xf0\xc7\x03Y\xcf\xd5\xd6\xe8\xa72\xb3\xe6\x98@\x86\xc5\xcd\'\x12\x8b\xcc{\xce\xb7\x11<b`\x07#>+@n\x94\x80\tm\xb6\xb3owo5\x08P\xfb\x02\x87\xc5>\x89\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TWCA Root Certification Authority O=TAIWAN-CA OU=Root CA
     * Subject: CN=TWCA Root Certification Authority O=TAIWAN-CA OU=Root CA
     * Label: "TWCA Root Certification Authority"
     * Serial: 1
     * MD5 Fingerprint: aa:08:8f:f6:f9:7b:b7:f2:b1:a7:1e:9b:ea:ea:bd:79
     * SHA1 Fingerprint: cf:9e:87:6d:d3:eb:fc:42:26:97:a3:b5:a3:7a:a0:76:a9:06:23:48
     * SHA256 Fingerprint: bf:d8:8f:e1:10:1c:41:ae:3e:80:1b:f8:be:56:35:0e:e9:ba:d1:a6:b9:bd:51:5e:dc:5c:6d:5b:87:11:ac:44
     * -----BEGIN CERTIFICATE-----
     * MIIDezCCAmOgAwIBAgIBATANBgkqhkiG9w0BAQUFADBfMQswCQYDVQQGEwJUVzES
     * MBAGA1UECgwJVEFJV0FOLUNBMRAwDgYDVQQLDAdSb290IENBMSowKAYDVQQDDCFU
     * V0NBIFJvb3QgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMDgwODI4MDcyNDMz
     * WhcNMzAxMjMxMTU1OTU5WjBfMQswCQYDVQQGEwJUVzESMBAGA1UECgwJVEFJV0FO
     * LUNBMRAwDgYDVQQLDAdSb290IENBMSowKAYDVQQDDCFUV0NBIFJvb3QgQ2VydGlm
     * aWNhdGlvbiBBdXRob3JpdHkwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIB
     * AQCwfnK4pAOU5qfeCTiRShFAh6d8WWQUe7UREN3+v9XAu1bihSX0NXIP+FPQQeFE
     * AcK0HMMxQhZHhTMidrIKbw/lJVBPhYa+v5guEGcevhEFhgWQxFnQfHgQsIBct+HH
     * K3XLfJ+utdGdIzdjp9xCoi2SBBtQwXu4PhvJVgSLL1KbralW6cH/ralYhzC2gfeX
     * RfwZVzsrb+RH9JlF/h3x+JejiB03HFyP4HYlmlD4oFT/RJB2I9IyxsOrBr/8+7/z
     * rX2SYgJbKdM1o5OaQ2RgXbL6Mv87BK9NQGr5x+PvI/1ry+UPizgN7gr8/g+YnzAx
     * 3WxSZfmLgb4i4RxYA7qRG4kHAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
     * HRMBAf8EBTADAQH/MB0GA1UdDgQWBBRqOFsmjd6LWvJPelSDGRjjCDWmujANBgkq
     * hkiG9w0BAQUFAAOCAQEAPNV3PdrfibqHDAhUaiBQkr6wQT25JmSDCi/oQMCXKCeC
     * MErJk/9q56YAf4lCmtYR5VPOL8zy2gXE/uJQxDqGfczafhAJO5I1KlOy/usrBdls
     * XebQ79NqZp4VKIV66IIArB6nCWlWQtNoURi+VJq/REG6Sb4gumlc7rh3zc5sH62D
     * lhh9DrUUOYTxKOkto557HnpyWoOzeW/vtPzQCqVYT0bf+215WfKEIlKuD8z7fDvn
     * aspHYcN6+NOSBB+4IIThNlQWx0DeO4pz3N/GCUzf7Nr/1FNCocnyYh0igzyXxfkZ
     * YiesZSLX0zzG5Y6yU8xJzrww/nsOM5D77dIUkR8Hrw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02TW1\x120\x10\x06\x03U\x04\n\x0c\tTAIWAN-CA1\x100\x0e\x06\x03U\x04\x0b\x0c\x07Root CA1*0(\x06\x03U\x04\x03\x0c!TWCA Root Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb0~r\xb8\xa4\x03\x94\xe6\xa7\xde\t8\x91J\x11@\x87\xa7|Yd\x14{\xb5\x11\x10\xdd\xfe\xbf\xd5\xc0\xbbV\xe2\x85%\xf45r\x0f\xf8S\xd0A\xe1D\x01\xc2\xb4\x1c\xc31B\x16G\x853\"v\xb2\no\x0f\xe5%PO\x85\x86\xbe\xbf\x98.\x10g\x1e\xbe\x11\x05\x86\x05\x90\xc4Y\xd0|x\x10\xb0\x80\\\xb7\xe1\xc7+u\xcb|\x9f\xae\xb5\xd1\x9d#7c\xa7\xdcB\xa2-\x92\x04\x1bP\xc1{\xb8>\x1b\xc9V\x04\x8b/R\x9b\xad\xa9V\xe9\xc1\xff\xad\xa9X\x870\xb6\x81\xf7\x97E\xfc\x19W;+o\xe4G\xf4\x99E\xfe\x1d\xf1\xf8\x97\xa3\x88\x1d7\x1c\\\x8f\xe0v%\x9aP\xf8\xa0T\xffD\x90v#\xd22\xc6\xc3\xab\x06\xbf\xfc\xfb\xbf\xf3\xad}\x92b\x02[)\xd35\xa3\x93\x9aCd`]\xb2\xfa2\xff;\x04\xafM@j\xf9\xc7\xe3\xef#\xfdk\xcb\xe5\x0f\x8b8\r\xee\n\xfc\xfe\x0f\x98\x9f01\xddlRe\xf9\x8b\x81\xbe\"\xe1\x1cX\x03\xba\x91\x1b\x89\x07\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GDCA TrustAUTH R5 ROOT O=GUANG DONG CERTIFICATE AUTHORITY CO.,LTD.
     * Subject: CN=GDCA TrustAUTH R5 ROOT O=GUANG DONG CERTIFICATE AUTHORITY CO.,LTD.
     * Label: "GDCA TrustAUTH R5 ROOT"
     * Serial: 9009899650740120186
     * MD5 Fingerprint: 63:cc:d9:3d:34:35:5c:6f:53:a3:e2:08:70:48:1f:b4
     * SHA1 Fingerprint: 0f:36:38:5b:81:1a:25:c3:9b:31:4e:83:ca:e9:34:66:70:cc:74:b4
     * SHA256 Fingerprint: bf:ff:8f:d0:44:33:48:7d:6a:8a:a6:0c:1a:29:76:7a:9f:c2:bb:b0:5e:42:0f:71:3a:13:b9:92:89:1d:38:93
     * -----BEGIN CERTIFICATE-----
     * MIIFiDCCA3CgAwIBAgIIfQmX/vBH6nowDQYJKoZIhvcNAQELBQAwYjELMAkGA1UE
     * BhMCQ04xMjAwBgNVBAoMKUdVQU5HIERPTkcgQ0VSVElGSUNBVEUgQVVUSE9SSVRZ
     * IENPLixMVEQuMR8wHQYDVQQDDBZHRENBIFRydXN0QVVUSCBSNSBST09UMB4XDTE0
     * MTEyNjA1MTMxNVoXDTQwMTIzMTE1NTk1OVowYjELMAkGA1UEBhMCQ04xMjAwBgNV
     * BAoMKUdVQU5HIERPTkcgQ0VSVElGSUNBVEUgQVVUSE9SSVRZIENPLixMVEQuMR8w
     * HQYDVQQDDBZHRENBIFRydXN0QVVUSCBSNSBST09UMIICIjANBgkqhkiG9w0BAQEF
     * AAOCAg8AMIICCgKCAgEA2aMW8Mh0dHeb7zMNOwZ+Vfy1YI92hhJCfVZmPoiC7XJj
     * Dp6L3TQsAlFRwxn9WVSEyfFrs0yw6ehGXTjGoqcuEVe6ghWinI9tsJlKCvLriXBj
     * TnnEt1u9ol2x8kECK62pOqPseQrsXzrj/e+APK00mxqriCZ7VqKChh/rNYmDf1+u
     * KU49tm7srsHwJ5uu4/Ts765/94Y9cnrrpftZTqfrlYwiOXnhLQiPzLyRuEH3FMEj
     * qcOtmkVEs7LXLM3GKeJQEK5cy4KOFxg2fZfmiJqwTTQJ9Cy5WmYqsBebnh52nUpm
     * MUHfP/vFBu8btn4aRjb3ZGM74zkYI+dndRTVdVeSN72+ahsmUPI2JgaQxXABZG12
     * ZuGR224HwGGALrIuL4xwp9E7PLOR5G62xDtw8mySlwnNR30YwPO7ng/Wi64HtloP
     * zgsMR6flPri9fcebNaBhlzpBdRfMK5Z3KpIhHtmVdiBnaM8Nvd/WHwlqmuLMc3Gk
     * L30SgLdTMEZeS1SZD2fJpcjyIMGC7J0R38IC+xo70e0gmu9lZJIQDSri3nDxGGeC
     * jGHeuLzRL5z7D9Ar7Rt2ueQ5Vfj4oR24qoAATILnsn8JuLwwoC8N9VKejveSswoA
     * HQBUlwbgsQfZxw9cZX08bVlX5O2ljelAU58VS6Bx9hoh49pwBiFYFIeFd3mqgnkC
     * AwEAAaNCMEAwHQYDVR0OBBYEFOLJQJ9NzuiaoXzPDj9lxSmIahlRMA8GA1UdEwEB
     * /wQFMAMBAf8wDgYDVR0PAQH/BAQDAgGGMA0GCSqGSIb3DQEBCwUAA4ICAQDRSVfg
     * p8xoWLoBDysZzY2wYUWsEe1jUGn4H3++Fo/9nesLqjJHdtJnJO29fDMylyrHBYZm
     * DRd9FBUb1Ov9H5r2XpdptxolpAqzkT9fNqyL7FeoPueBihhXOYV0GkLH6VsTX4/5
     * COmSdI31R9KrO9b7eGZONn356ZLpBN79SWP8bfsUcZNnL0dKt7n/HipzcEYwv1ry
     * L3ml4Y0M2fmyYzeMN2WFcGpcWwlyua1jPLHd+PwyvzeG5LuOmCd+uh8W4XAR8gPf
     * JWIyJyYYMoSf/wA6E7qaTfRPuBRwIrHKK5DOKcFw9C+df/KQHtZa37dG/OaG+svg
     * IHZ6uqbL9XzeYqWxi+7egmaKTjowHz+Ay60nugxe19CxVsp3cbK1daFQqUBDF8Io
     * 2c9Si1vIY9RCPqAzekYu9wogRlR+ak8x8YF+QnQ4ZXMn7sZ8uI7XpTrXmKGcjBBV
     * 09tL7ECQ8s1uV9JiDnxXk7Gnbc2dg7sq5+W2O3FYrf3RRbxake5TFW/TRQl1brqQ
     * XR4EzzffHqhmsYzmIGrv/EhOdJhCrylvLmrH+33RZjEizIYAfmaDDEL0vTSSwxrq
     * T8p+ck0LcIymSLumoRT2+1hEmRSuqguTaaApJUqlyyvdimYHFngVV3Eb7PVHhPOe
     * MTd61X8kreS8/f3MboPoDKi3QWwH3b08hpcv0g==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CN1200\x06\x03U\x04\n\x0c)GUANG DONG CERTIFICATE AUTHORITY CO.,LTD.1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16GDCA TrustAUTH R5 ROOT",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xd9\xa3\x16\xf0\xc8ttw\x9b\xef3\r;\x06~U\xfc\xb5`\x8fv\x86\x12B}Vf>\x88\x82\xedrc\x0e\x9e\x8b\xdd4,\x02QQ\xc3\x19\xfdYT\x84\xc9\xf1k\xb3L\xb0\xe9\xe8F]8\xc6\xa2\xa7.\x11W\xba\x82\x15\xa2\x9c\x8fm\xb0\x99J\n\xf2\xeb\x89pcNy\xc4\xb7[\xbd\xa2]\xb1\xf2A\x02+\xad\xa9:\xa3\xecy\n\xec_:\xe3\xfd\xef\x80<\xad4\x9b\x1a\xab\x88&{V\xa2\x82\x86\x1f\xeb5\x89\x83\x7f_\xae)N=\xb6n\xec\xae\xc1\xf0\'\x9b\xae\xe3\xf4\xec\xef\xae\x7f\xf7\x86=rz\xeb\xa5\xfbYN\xa7\xeb\x95\x8c\"9y\xe1-\x08\x8f\xcc\xbc\x91\xb8A\xf7\x14\xc1#\xa9\xc3\xad\x9aED\xb3\xb2\xd7,\xcd\xc6)\xe2P\x10\xae\\\xcb\x82\x8e\x17\x186}\x97\xe6\x88\x9a\xb0M4\t\xf4,\xb9Zf*\xb0\x17\x9b\x9e\x1ev\x9dJf1A\xdf?\xfb\xc5\x06\xef\x1b\xb6~\x1aF6\xf7dc;\xe39\x18#\xe7gu\x14\xd5uW\x927\xbd\xbej\x1b&P\xf26&\x06\x90\xc5p\x01dmvf\xe1\x91\xdbn\x07\xc0a\x80.\xb2./\x8cp\xa7\xd1;<\xb3\x91\xe4n\xb6\xc4;p\xf2l\x92\x97\t\xcdG}\x18\xc0\xf3\xbb\x9e\x0f\xd6\x8b\xae\x07\xb6Z\x0f\xce\x0b\x0cG\xa7\xe5>\xb8\xbd}\xc7\x9b5\xa0a\x97:Au\x17\xcc+\x96w*\x92!\x1e\xd9\x95v gh\xcf\r\xbd\xdf\xd6\x1f\tj\x9a\xe2\xccsq\xa4/}\x12\x80\xb7S0F^KT\x99\x0fg\xc9\xa5\xc8\xf2 \xc1\x82\xec\x9d\x11\xdf\xc2\x02\xfb\x1a;\xd1\xed \x9a\xefed\x92\x10\r*\xe2\xdep\xf1\x18g\x82\x8ca\xde\xb8\xbc\xd1/\x9c\xfb\x0f\xd0+\xed\x1bv\xb9\xe49U\xf8\xf8\xa1\x1d\xb8\xaa\x80\x00L\x82\xe7\xb2\x7f\t\xb8\xbc0\xa0/\r\xf5R\x9e\x8e\xf7\x92\xb3\n\x00\x1d\x00T\x97\x06\xe0\xb1\x07\xd9\xc7\x0f\\e}<mYW\xe4\xed\xa5\x8d\xe9@S\x9f\x15K\xa0q\xf6\x1a!\xe3\xdap\x06!X\x14\x87\x85wy\xaa\x82y\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=Chunghwa Telecom Co., Ltd. OU=ePKI Root Certification Authority
     * Subject: O=Chunghwa Telecom Co., Ltd. OU=ePKI Root Certification Authority
     * Label: "ePKI Root Certification Authority"
     * Serial: 28956088682735189655030529057352760477
     * MD5 Fingerprint: 1b:2e:00:ca:26:06:90:3d:ad:fe:6f:15:68:d3:6b:b3
     * SHA1 Fingerprint: 67:65:0d:f1:7e:8e:7e:5b:82:40:a4:f4:56:4b:cf:e2:3d:69:c6:f0
     * SHA256 Fingerprint: c0:a6:f4:dc:63:a2:4b:fd:cf:54:ef:2a:6a:08:2a:0a:72:de:35:80:3e:2f:f5:ff:52:7a:e5:d8:72:06:df:d5
     * -----BEGIN CERTIFICATE-----
     * MIIFsDCCA5igAwIBAgIQFci9ZUdcr7iXAF7kBtK8nTANBgkqhkiG9w0BAQUFADBe
     * MQswCQYDVQQGEwJUVzEjMCEGA1UECgwaQ2h1bmdod2EgVGVsZWNvbSBDby4sIEx0
     * ZC4xKjAoBgNVBAsMIWVQS0kgUm9vdCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTAe
     * Fw0wNDEyMjAwMjMxMjdaFw0zNDEyMjAwMjMxMjdaMF4xCzAJBgNVBAYTAlRXMSMw
     * IQYDVQQKDBpDaHVuZ2h3YSBUZWxlY29tIENvLiwgTHRkLjEqMCgGA1UECwwhZVBL
     * SSBSb290IENlcnRpZmljYXRpb24gQXV0aG9yaXR5MIICIjANBgkqhkiG9w0BAQEF
     * AAOCAg8AMIICCgKCAgEA4SUP7o3biDN1Z82tH306Tm2d0y8U82N0ywEhajfqhFAH
     * SyZbCUNsIZ5qyNUD9WBpj8zwIuQf5/dqIjG3LBXy4P4AakP/h2XGtRrBp0xtInAh
     * ijHyl3SJCRImHJ7K2RKilTza6We/CKBk49ZCt0Xvl/T29de1ShUCWH2YWEtgvM3X
     * DZoTM1PRYfl61dd4s5oz9wCGzh1NlDivqOx4UXCKXBCDUSH3ET00hl7lSM2XgYI1
     * TBnsZfZrxQWh7kcT1rMhJ5QQCtkkO7q+RBNGMD+XPNjX12ruOzjjK9SXDrkb5wdJ
     * fzcq+Xd4z1TtW0ado4AOkUPB1ltfFLqfpo0kR0BZv3I4sjZsN/+Z0V0OWQqraffA
     * sgRFelQArr5T9rXn4fg8ozHSqf4hUmTFpmfwdQcGlBSBVcYn5AGPF8Fqcde+S/uU
     * WH1+ETOxQvdibBjWzwloPn9s9h6PYq2lY9sJpx8iQkEeb5mKPtf5P0B6ebClAZLS
     * nT0IFaUQAS2zMnaolQ2zepr7BxB4EW/hj8e6DyUadCrlHJhBmd8hh+iVBmoKs2pH
     * dmX2Os+PYhcZewoozRrSgx4hxyy/vv9haLdnG7t4TY3OZ+XkwY63I2binZB1NJip
     * NiuKmpS5nezMirH4JYlcWrYvjB9teSSnUmjDhDXiZo1jDiVN1Rmy5nk3pyKdVDEC
     * AwEAAaNqMGgwHQYDVR0OBBYEFB4M97Zn8uGSJglFwFU5Lnc/QkqiMAwGA1UdEwQF
     * MAMBAf8wOQYEZyoHAAQxMC8wLQIBADAJBgUrDgMCGgUAMAcGBWcqAwAABBRFsMLH
     * ClZ87lt4DJX5GFPBphzYEDANBgkqhkiG9w0BAQUFAAOCAgEACbODU1kBPpVJufGB
     * uvl2ICO1J2B01GqZNF5sAFPZn/KmsSQHRGoqxqWOeBLoR9lYGxMqXnmbnwoqZ6Yl
     * PwZpVnPDimZI+ymBV3QGypzqKOg4ZyYr8dW1P2WT+DZdjo2NQCCHGervJ8A9tDkP
     * JXtoUHRVnAxZfVo9QZQlUgjgRywVMRnVvwdVxrsStZf0X4OFunHB2WyBEXYKCrC/
     * gpf36j36+uwtqSiUO1bd0lEursC9CBWMd1I0ltabrNMdjmEPNXubrjlpC2JgQCA2
     * j6/7Nu4tCEoduL+bXPjqpRugc6bY+G7gMwRfaKonh+3ZwZCc7b3jajWvY9+rGNm6
     * 5ulK6lCKD2GTHuItGeIwlDWSXQ62B68ZgI9HkFFLLk3dheLSClIKF5r8GrBQAuUB
     * o2M3IUxExJtRmREOc5wGj1QupyheRDmHVi03vYVElOEMSyycw5KFNGHLD7ibSkNS
     * /jQ6fbjpKdx2qcgw+BRxgMYeNkh0IkFch4LoGHGLQYlE535YW6i4jRPpp2zDR+2z
     * Gp1iro2C6pSe3VkQw63d4k3jMdXH7OjysP6SHhYKGvzZ8/gntsm+HbRsZJB/9OTE
     * W9c3rkIO3aQab3yIVMUWbuF6aC74Or8NpDyJO3inTmODBCEIZ43ygknQW/2xzQ+D
     * hNQ+IIX3Sj0rnP0qCglN6oH4EZw=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02TW1#0!\x06\x03U\x04\n\x0c\x1aChunghwa Telecom Co., Ltd.1*0(\x06\x03U\x04\x0b\x0c!ePKI Root Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xe1%\x0f\xee\x8d\xdb\x883ug\xcd\xad\x1f}:Nm\x9d\xd3/\x14\xf3ct\xcb\x01!j7\xea\x84P\x07K&[\tCl!\x9ej\xc8\xd5\x03\xf5`i\x8f\xcc\xf0\"\xe4\x1f\xe7\xf7j\"1\xb7,\x15\xf2\xe0\xfe\x00jC\xff\x87e\xc6\xb5\x1a\xc1\xa7Lm\"p!\x8a1\xf2\x97t\x89\t\x12&\x1c\x9e\xca\xd9\x12\xa2\x95<\xda\xe9g\xbf\x08\xa0d\xe3\xd6B\xb7E\xef\x97\xf4\xf6\xf5\xd7\xb5J\x15\x02X}\x98XK`\xbc\xcd\xd7\r\x9a\x133S\xd1a\xf9z\xd5\xd7x\xb3\x9a3\xf7\x00\x86\xce\x1dM\x948\xaf\xa8\xecxQp\x8a\\\x10\x83Q!\xf7\x11=4\x86^\xe5H\xcd\x97\x81\x825L\x19\xece\xf6k\xc5\x05\xa1\xeeG\x13\xd6\xb3!\'\x94\x10\n\xd9$;\xba\xbeD\x13F0?\x97<\xd8\xd7\xd7j\xee;8\xe3+\xd4\x97\x0e\xb9\x1b\xe7\x07I\x7f7*\xf9wx\xcfT\xed[F\x9d\xa3\x80\x0e\x91C\xc1\xd6[_\x14\xba\x9f\xa6\x8d$G@Y\xbfr8\xb26l7\xff\x99\xd1]\x0eY\n\xabi\xf7\xc0\xb2\x04EzT\x00\xae\xbeS\xf6\xb5\xe7\xe1\xf8<\xa31\xd2\xa9\xfe!Rd\xc5\xa6g\xf0u\x07\x06\x94\x14\x81U\xc6\'\xe4\x01\x8f\x17\xc1jq\xd7\xbeK\xfb\x94X}~\x113\xb1B\xf7bl\x18\xd6\xcf\th>\x7fl\xf6\x1e\x8fb\xad\xa5c\xdb\t\xa7\x1f\"BA\x1eo\x99\x8a>\xd7\xf9?@zy\xb0\xa5\x01\x92\xd2\x9d=\x08\x15\xa5\x10\x01-\xb32v\xa8\x95\r\xb3z\x9a\xfb\x07\x10x\x11o\xe1\x8f\xc7\xba\x0f%\x1at*\xe5\x1c\x98A\x99\xdf!\x87\xe8\x95\x06j\n\xb3jGve\xf6:\xcf\x8fb\x17\x19{\n(\xcd\x1a\xd2\x83\x1e!\xc7,\xbf\xbe\xffah\xb7g\x1b\xbbxM\x8d\xceg\xe5\xe4\xc1\x8e\xb7#f\xe2\x9d\x90u4\x98\xa96+\x8a\x9a\x94\xb9\x9d\xec\xcc\x8a\xb1\xf8%\x89\\Z\xb6/\x8c\x1fmy$\xa7Rh\xc3\x845\xe2f\x8dc\x0e%M\xd5\x19\xb2\xe6y7\xa7\"\x9dT1\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=Trustis Limited OU=Trustis FPS Root CA
     * Subject: O=Trustis Limited OU=Trustis FPS Root CA
     * Label: "Trustis FPS Root CA"
     * Serial: 36053640375399034304724988975563710553
     * MD5 Fingerprint: 30:c9:e7:1e:6b:e6:14:eb:65:b2:16:69:20:31:67:4d
     * SHA1 Fingerprint: 3b:c0:38:0b:33:c3:f6:a6:0c:86:15:22:93:d9:df:f5:4b:81:c0:04
     * SHA256 Fingerprint: c1:b4:82:99:ab:a5:20:8f:e9:63:0a:ce:55:ca:68:a0:3e:da:5a:51:9c:88:02:a0:d3:a6:73:be:8f:8e:55:7d
     * -----BEGIN CERTIFICATE-----
     * MIIDZzCCAk+gAwIBAgIQGx+ttiD5JNM2a/fH8YygWTANBgkqhkiG9w0BAQUFADBF
     * MQswCQYDVQQGEwJHQjEYMBYGA1UEChMPVHJ1c3RpcyBMaW1pdGVkMRwwGgYDVQQL
     * ExNUcnVzdGlzIEZQUyBSb290IENBMB4XDTAzMTIyMzEyMTQwNloXDTI0MDEyMTEx
     * MzY1NFowRTELMAkGA1UEBhMCR0IxGDAWBgNVBAoTD1RydXN0aXMgTGltaXRlZDEc
     * MBoGA1UECxMTVHJ1c3RpcyBGUFMgUm9vdCBDQTCCASIwDQYJKoZIhvcNAQEBBQAD
     * ggEPADCCAQoCggEBAMVQe547NdDfxIzNjpvto8A2mfRC6qc+gIMPpqdZh8mQRUN+
     * AOqGeSoDvT03mYlmt+WKVoaTnGhLaASMk5MCPjDSNzoiYYkchU59j9WvezX2fihH
     * iTHcDnlkH5nSW7r+f2C/revnPDgpai/lkQtV/+xvWNUtyd5MZnGPDNcE2gfmHhjj
     * vSkCqPoc4Vu5g6hBSLwacY3nYuUtsuvffM/bq1rKMfFMIvMFE/eC+XN5DL7XSxzA
     * 0RU8k0Fk0ea+IxciAIleH2ulrG6nS4zto3Lmr2NNL4XSFDWaLk6M6jKYKIahkQlB
     * OrTh4/L68MkKokHdqeMDx4gVOxzUGpTXn2RZEm0CAwEAAaNTMFEwDwYDVR0TAQH/
     * BAUwAwEB/zAfBgNVHSMEGDAWgBS6+nEleYtXQSUhhgtx67JkDoshZzAdBgNVHQ4E
     * FgQUuvpxJXmLV0ElIYYLceuyZA6LIWcwDQYJKoZIhvcNAQEFBQADggEBAH5Y//01
     * GX2cGE+esCu8jowU/yyg2kdbw++BLa8F6nRIW/M+TgfHbcWzk88iNVy2P3UnXwmW
     * zaD+vkAMXBJV+JOCyinpXj9WV4s4NvdFGkwozZ5BuO1WTISkQMi4sKUraXAEasP4
     * 1BIy+Q7DsdwyhEQsb8tGD+pmQQ9P8Vilpg0ND2HepZ5dfWWhPBfnqFVO76DH7cZE
     * f1T1o+CP8HxVIo8ptoGj4W1OLBuAZ+ytIJ8MYmHVl/9D7S3B2l0pKoU/rGXuhg8F
     * jZBf3+6f9L/uHfuY5H+QK4R4EA5sSVPvFVtlRkpdr7r7OnIdzfYliB6XzCGcKQEN
     * ZetX2fNXlrtIzYE=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GB1\x180\x16\x06\x03U\x04\n\x13\x0fTrustis Limited1\x1c0\x1a\x06\x03U\x04\x0b\x13\x13Trustis FPS Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xc5P{\x9e;5\xd0\xdf\xc4\x8c\xcd\x8e\x9b\xed\xa3\xc06\x99\xf4B\xea\xa7>\x80\x83\x0f\xa6\xa7Y\x87\xc9\x90EC~\x00\xea\x86y*\x03\xbd=7\x99\x89f\xb7\xe5\x8aV\x86\x93\x9chKh\x04\x8c\x93\x93\x02>0\xd27:\"a\x89\x1c\x85N}\x8f\xd5\xaf{5\xf6~(G\x891\xdc\x0eyd\x1f\x99\xd2[\xba\xfe\x7f`\xbf\xad\xeb\xe7<8)j/\xe5\x91\x0bU\xff\xecoX\xd5-\xc9\xdeLfq\x8f\x0c\xd7\x04\xda\x07\xe6\x1e\x18\xe3\xbd)\x02\xa8\xfa\x1c\xe1[\xb9\x83\xa8AH\xbc\x1aq\x8d\xe7b\xe5-\xb2\xeb\xdf|\xcf\xdb\xabZ\xca1\xf1L\"\xf3\x05\x13\xf7\x82\xf9sy\x0c\xbe\xd7K\x1c\xc0\xd1\x15<\x93Ad\xd1\xe6\xbe#\x17\"\x00\x89^\x1fk\xa5\xacn\xa7K\x8c\xed\xa3r\xe6\xafcM/\x85\xd2\x145\x9a.N\x8c\xea2\x98(\x86\xa1\x91\tA:\xb4\xe1\xe3\xf2\xfa\xf0\xc9\n\xa2A\xdd\xa9\xe3\x03\xc7\x88\x15;\x1c\xd4\x1a\x94\xd7\x9fdY\x12m\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=The Go Daddy Group, Inc. OU=Go Daddy Class 2 Certification Authority
     * Subject: O=The Go Daddy Group, Inc. OU=Go Daddy Class 2 Certification Authority
     * Label: "Go Daddy Class 2 CA"
     * Serial: 0
     * MD5 Fingerprint: 91:de:06:25:ab:da:fd:32:17:0c:bb:25:17:2a:84:67
     * SHA1 Fingerprint: 27:96:ba:e6:3f:18:01:e2:77:26:1b:a0:d7:77:70:02:8f:20:ee:e4
     * SHA256 Fingerprint: c3:84:6b:f2:4b:9e:93:ca:64:27:4c:0e:c6:7c:1e:cc:5e:02:4f:fc:ac:d2:d7:40:19:35:0e:81:fe:54:6a:e4
     * -----BEGIN CERTIFICATE-----
     * MIIEADCCAuigAwIBAgIBADANBgkqhkiG9w0BAQUFADBjMQswCQYDVQQGEwJVUzEh
     * MB8GA1UEChMYVGhlIEdvIERhZGR5IEdyb3VwLCBJbmMuMTEwLwYDVQQLEyhHbyBE
     * YWRkeSBDbGFzcyAyIENlcnRpZmljYXRpb24gQXV0aG9yaXR5MB4XDTA0MDYyOTE3
     * MDYyMFoXDTM0MDYyOTE3MDYyMFowYzELMAkGA1UEBhMCVVMxITAfBgNVBAoTGFRo
     * ZSBHbyBEYWRkeSBHcm91cCwgSW5jLjExMC8GA1UECxMoR28gRGFkZHkgQ2xhc3Mg
     * MiBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eTCCASAwDQYJKoZIhvcNAQEBBQADggEN
     * ADCCAQgCggEBAN6d1+pXGEmhW+vXX0iG6r7d/+TvZxz0ZWizV3GgXne77ZtJ6XCA
     * PVYYYwhv2vLM0D9/AlQiVBDYsoHUwHU9S3/Hd8M+eKsaA7Ugay9qK7HFiH7Eux6w
     * wdhFJ2+qN1j3hybX2C32qRe3H3I2TqYXP2WYktsqbl2i/ojgC95/5Y0V4evLOtXi
     * EqITLdiOr18SPaAIBQi2XKVlOARFmR6jYGB0xUGlcmIbYsUfb18aQr4CUWWoriMY
     * avx4A6lNf4DD+qta/KFApMoZFv6yyO9ecw3ud72a9nmYvLEHZ6IVDd2gWMZEewo+
     * YihfukEHU1jPEX44dMX4/7VpkI+EdOqXG68CAQOjgcAwgb0wHQYDVR0OBBYEFNLE
     * sNKR1EwRcbNhyz2h/t2oatTjMIGNBgNVHSMEgYUwgYKAFNLEsNKR1EwRcbNhyz2h
     * /t2oatTjoWekZTBjMQswCQYDVQQGEwJVUzEhMB8GA1UEChMYVGhlIEdvIERhZGR5
     * IEdyb3VwLCBJbmMuMTEwLwYDVQQLEyhHbyBEYWRkeSBDbGFzcyAyIENlcnRpZmlj
     * YXRpb24gQXV0aG9yaXR5ggEAMAwGA1UdEwQFMAMBAf8wDQYJKoZIhvcNAQEFBQAD
     * ggEBADJL87LKPpH8EsahB4yOd6AzBhRckB4Y9wimPQoZ+YeAEW5p5JYXMP80kWNy
     * OO7MHAGjHZQopDH2esRU1/blMVgDoszOYtuURXO1v0XJJLXVggKtI3lpjbi2Tc7P
     * TMozI+gciKqdi0FuFskg5YmezTvacPd+mSYgFFQlq25zheabIZ0KbIIOqPjCDPoQ
     * HmyW74cNxA9hi63ugyuV+I6ShHI56yDqg+2DzZduCLzrTia2cyvk0/ZM/iZx4mER
     * dEr/VxqHD3VILs9RaRegAhJhldXRQLIQTO7ErBBDpqWeCtWVYpoNz4iCxTIM5Cuf
     * ReYNnyicsbkqWletNw+vHX/bvZ8=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1!0\x1f\x06\x03U\x04\n\x13\x18The Go Daddy Group, Inc.110/\x06\x03U\x04\x0b\x13(Go Daddy Class 2 Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\r\x000\x82\x01\x08\x02\x82\x01\x01\x00\xde\x9d\xd7\xeaW\x18I\xa1[\xeb\xd7_H\x86\xea\xbe\xdd\xff\xe4\xefg\x1c\xf4eh\xb3Wq\xa0^w\xbb\xed\x9bI\xe9p\x80=V\x18c\x08o\xda\xf2\xcc\xd0?\x7f\x02T\"T\x10\xd8\xb2\x81\xd4\xc0u=K\x7f\xc7w\xc3>x\xab\x1a\x03\xb5 k/j+\xb1\xc5\x88~\xc4\xbb\x1e\xb0\xc1\xd8E\'o\xaa7X\xf7\x87&\xd7\xd8-\xf6\xa9\x17\xb7\x1fr6N\xa6\x17?e\x98\x92\xdb*n]\xa2\xfe\x88\xe0\x0b\xde\x7f\xe5\x8d\x15\xe1\xeb\xcb:\xd5\xe2\x12\xa2\x13-\xd8\x8e\xaf_\x12=\xa0\x08\x05\x08\xb6\\\xa5e8\x04E\x99\x1e\xa3``t\xc5A\xa5rb\x1bb\xc5\x1fo_\x1aB\xbe\x02Qe\xa8\xae#\x18j\xfcx\x03\xa9M\x7f\x80\xc3\xfa\xabZ\xfc\xa1@\xa4\xca\x19\x16\xfe\xb2\xc8\xef^s\r\xeew\xbd\x9a\xf6y\x98\xbc\xb1\x07g\xa2\x15\r\xdd\xa0X\xc6D{\n>b(_\xbaA\x07SX\xcf\x11~8t\xc5\xf8\xff\xb5i\x90\x8f\x84t\xea\x97\x1b\xaf\x02\x01\x03",
        name_constraints: None
    },

    /*
     * Issuer: CN=GTS Root R2 O=Google Trust Services LLC
     * Subject: CN=GTS Root R2 O=Google Trust Services LLC
     * Label: "GTS Root R2"
     * Serial: 146587176055767053814479386953112547951
     * MD5 Fingerprint: 44:ed:9a:0e:a4:09:3b:00:f2:ae:4c:a3:c6:61:b0:8b
     * SHA1 Fingerprint: d2:73:96:2a:2a:5e:39:9f:73:3f:e1:c7:1e:64:3f:03:38:34:fc:4d
     * SHA256 Fingerprint: c4:5d:7b:b0:8e:6d:67:e6:2e:42:35:11:0b:56:4e:5f:78:fd:92:ef:05:8c:84:0a:ea:4e:64:55:d7:58:5c:60
     * -----BEGIN CERTIFICATE-----
     * MIIFWjCCA0KgAwIBAgIQbkepxlqz5yDFMJo/aFLybzANBgkqhkiG9w0BAQwFADBH
     * MQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNlcnZpY2VzIExM
     * QzEUMBIGA1UEAxMLR1RTIFJvb3QgUjIwHhcNMTYwNjIyMDAwMDAwWhcNMzYwNjIy
     * MDAwMDAwWjBHMQswCQYDVQQGEwJVUzEiMCAGA1UEChMZR29vZ2xlIFRydXN0IFNl
     * cnZpY2VzIExMQzEUMBIGA1UEAxMLR1RTIFJvb3QgUjIwggIiMA0GCSqGSIb3DQEB
     * AQUAA4ICDwAwggIKAoICAQDO3v2m++zsFDQ8BwZabFn3GTXd98GdVarTzTukk3Lv
     * CvptnfbwhYBboUhSnznFt+4orO/LdmgUud+tAWyZH8QiHZ/+cnfgLFuv5AS/T3Kg
     * GjSY6Dlo7JUle3ah5mm5hRm9iYz+re026nO8/4Piy33B0s5Ks40FnotJk9/BW9Bu
     * XvAuMC6C/Pq8tBcKSOWIm8Wba96wyrQD8Nr0kLhlZPdcTK3ofmZemde4wj7I0BOd
     * re7kRXuJVfeKH2JShBKzwkCX44ofR5GmdFrS+LFjKBC4swm4VndAoiaYecb+3yXu
     * PuWgf9RhD1FLPD+M2uFwdNjCaKH5wQzpoeJ/u1U8dgbuak7MkogwTZq9TwtImoS1
     * mKPV+3PBV2HdKFZ1E66HjucMUQkQdYhMvI35ezzUIkgfKtzra7tEscszcTJGr61K
     * 8YzodDqs5xoic4DSMPclQsciOzsSrZYuxsN2B6ogtzVJV+mSSeh2FnIxZyuWfoqj
     * x5RWIr9qS34BIbIjMt/kmkRtWVtd9QCgHJvGeJeNkP+byKq0rxFROV7Z+2et1VsR
     * nTKaG73VululycslaVNVJ1zgyjbLiGH7HrfQy+4W+9OmTN6SpdTi3/UGVN4unUu0
     * kzCqgc7dGtxRcw1PcOnlthYhGXmy5okLdWTK1au8CcEYof/UVKGFPP0UJAOyh9Ok
     * twIDAQABo0IwQDAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNV
     * HQ4EFgQUu//KjiOfT5nK2+JopqUVJxce2Q4wDQYJKoZIhvcNAQEMBQADggIBALZp
     * 8KZ3/p7uC4Gt4cCpx/k1HUCCq+YEtN/L9x0Pg/B+E02NjO7jMyLDOfxA325BS0JT
     * vhaI8dI4XsRomRyYUpOM52jtG2pzegVATX9lO9ZY8c6DR2Dj/5epnGB3GFW1fgiT
     * z9D2PGcDFWEJ+YF59exTpJ/JjwGLc8R3dtyDovUMSRqodt6Sm2T4syzFJ9MHwAiA
     * pJiS4wGWAqoC7o87xdFtCjMwc3i5T1QWvwsHoaRc5svJXISPD+AVdyx+Jn7axEvb
     * pxZ3B7DNdehyQtaVhJ2Gg/LkkM0JR9SLA3DaWsYDQvTtN6LwG1BUSw7YhN4ZKJmB
     * R64JGz9I0cNv4rBgF/XuIwKl2gBbbZCr7qLpGzvpx0QnRY5rn/WkhLx3+WuXrD5R
     * RaIRpsyF7gpo8j5QOHokYh4XIDdtak23CZvJ/KRY9bb7nE4Yu5UC56GtmwfuNmsk
     * 0jmGwZODUNKBRqhfYlcsu2xkiAhu7xNUX90txGdj08+JN7+dIPT7eoOboB6BAFDC
     * 5AwiWVIQ7UNWhwD4FFKnHYuTjKJNRn8nxnGbJN7k2oaLDX5rIMHAnuFl2GqjpuiF
     * izoHCBy69Y9Vmhh1fuXsgWbRIXOhNUQLgD1bnF5vKheW0YMjiGZt5obicDIvUiLn
     * yOd/xCxgXS/Dr55FBcOEArf9LAhST4Ldo/DUhgkC
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\"0 \x06\x03U\x04\n\x13\x19Google Trust Services LLC1\x140\x12\x06\x03U\x04\x03\x13\x0bGTS Root R2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xce\xde\xfd\xa6\xfb\xec\xec\x144<\x07\x06ZlY\xf7\x195\xdd\xf7\xc1\x9dU\xaa\xd3\xcd;\xa4\x93r\xef\n\xfam\x9d\xf6\xf0\x85\x80[\xa1HR\x9f9\xc5\xb7\xee(\xac\xef\xcbvh\x14\xb9\xdf\xad\x01l\x99\x1f\xc4\"\x1d\x9f\xferw\xe0,[\xaf\xe4\x04\xbfOr\xa0\x1a4\x98\xe89h\xec\x95%{v\xa1\xe6i\xb9\x85\x19\xbd\x89\x8c\xfe\xad\xed6\xeas\xbc\xff\x83\xe2\xcb}\xc1\xd2\xceJ\xb3\x8d\x05\x9e\x8bI\x93\xdf\xc1[\xd0n^\xf0.0.\x82\xfc\xfa\xbc\xb4\x17\nH\xe5\x88\x9b\xc5\x9bk\xde\xb0\xca\xb4\x03\xf0\xda\xf4\x90\xb8ed\xf7\\L\xad\xe8~f^\x99\xd7\xb8\xc2>\xc8\xd0\x13\x9d\xad\xee\xe4E{\x89U\xf7\x8a\x1fbR\x84\x12\xb3\xc2@\x97\xe3\x8a\x1fG\x91\xa6tZ\xd2\xf8\xb1c(\x10\xb8\xb3\t\xb8Vw@\xa2&\x98y\xc6\xfe\xdf%\xee>\xe5\xa0\x7f\xd4a\x0fQK<?\x8c\xda\xe1pt\xd8\xc2h\xa1\xf9\xc1\x0c\xe9\xa1\xe2\x7f\xbbU<v\x06\xeejN\xcc\x92\x880M\x9a\xbdO\x0bH\x9a\x84\xb5\x98\xa3\xd5\xfbs\xc1Wa\xdd(Vu\x13\xae\x87\x8e\xe7\x0cQ\t\x10u\x88L\xbc\x8d\xf9{<\xd4\"H\x1f*\xdc\xebk\xbbD\xb1\xcb3q2F\xaf\xadJ\xf1\x8c\xe8t:\xac\xe7\x1a\"s\x80\xd20\xf7%B\xc7\";;\x12\xad\x96.\xc6\xc3v\x07\xaa \xb75IW\xe9\x92I\xe8v\x16r1g+\x96~\x8a\xa3\xc7\x94V\"\xbfjK~\x01!\xb2#2\xdf\xe4\x9aDmY[]\xf5\x00\xa0\x1c\x9b\xc6x\x97\x8d\x90\xff\x9b\xc8\xaa\xb4\xaf\x11Q9^\xd9\xfbg\xad\xd5[\x11\x9d2\x9a\x1b\xbd\xd5\xba[\xa5\xc9\xcb%iSU\'\\\xe0\xca6\xcb\x88a\xfb\x1e\xb7\xd0\xcb\xee\x16\xfb\xd3\xa6L\xde\x92\xa5\xd4\xe2\xdf\xf5\x06T\xde.\x9dK\xb4\x930\xaa\x81\xce\xdd\x1a\xdcQs\rOp\xe9\xe5\xb6\x16!\x19y\xb2\xe6\x89\x0bud\xca\xd5\xab\xbc\t\xc1\x18\xa1\xff\xd4T\xa1\x85<\xfd\x14$\x03\xb2\x87\xd3\xa4\xb7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Microsoft RSA Root Certificate Authority 2017 O=Microsoft Corporation
     * Subject: CN=Microsoft RSA Root Certificate Authority 2017 O=Microsoft Corporation
     * Label: "Microsoft RSA Root Certificate Authority 2017"
     * Serial: 40975477897264996090493496164228220339
     * MD5 Fingerprint: 10:ff:00:ff:cf:c9:f8:c7:7a:c0:ee:35:8e:c9:0f:47
     * SHA1 Fingerprint: 73:a5:e6:4a:3b:ff:83:16:ff:0e:dc:cc:61:8a:90:6e:4e:ae:4d:74
     * SHA256 Fingerprint: c7:41:f7:0f:4b:2a:8d:88:bf:2e:71:c1:41:22:ef:53:ef:10:eb:a0:cf:a5:e6:4c:fa:20:f4:18:85:30:73:e0
     * -----BEGIN CERTIFICATE-----
     * MIIFqDCCA5CgAwIBAgIQHtOXCV/YtLNHcB6qvn9FszANBgkqhkiG9w0BAQwFADBl
     * MQswCQYDVQQGEwJVUzEeMBwGA1UEChMVTWljcm9zb2Z0IENvcnBvcmF0aW9uMTYw
     * NAYDVQQDEy1NaWNyb3NvZnQgUlNBIFJvb3QgQ2VydGlmaWNhdGUgQXV0aG9yaXR5
     * IDIwMTcwHhcNMTkxMjE4MjI1MTIyWhcNNDIwNzE4MjMwMDIzWjBlMQswCQYDVQQG
     * EwJVUzEeMBwGA1UEChMVTWljcm9zb2Z0IENvcnBvcmF0aW9uMTYwNAYDVQQDEy1N
     * aWNyb3NvZnQgUlNBIFJvb3QgQ2VydGlmaWNhdGUgQXV0aG9yaXR5IDIwMTcwggIi
     * MA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQDKW76UM4wplZEWCpW9R2LBifOZ
     * Nt9GkMml7Xhqb0eRaPgnZ1AzHaGm++DlQ6OEAlcBXZxIQIJTELy/xztokLaCLeX0
     * ZdDMbRnMlfl7rEqUrQ7eS0MdhweSE5CAg2Q1OQT85elss7YfUJQ4ZVBcF0a5toW1
     * HLUX6NZFndiyJrDKxHBKrmCk3bPZ7Pw71VdyvD/IybLeS2v4I2wDwAW9lcfNcztm
     * gGTjGqwu+UcF8ga2m3P1eDNbx6H7JyqhtJqRjJHTOoI+dkC0zVJhUXAoP8XFWvLJ
     * jEm7FFtNyP9nTUwSlq31/niol4fX/V4ggNyhSyL71Imtus5Hl0dVe49FyGcohJUc
     * aDDv70ngNXtk55iwlNpNhTs+VcQor1fznhPbRiefHqJeRIOkpcrVE7NLP8TjwuaG
     * YaRSMLl6IE9vDzhTyzMMEyuP1pq9KsgtsRx9S1HKR9FIJ3Jdh+vVReZIZZ2vUpC6
     * W6IYZVcSn2i51BVrlMRpIpj0M+Dt+VGOQVDJNE92kKz8OMHY4Xu54+OU4UZpyw4K
     * UGsTuqwPN1q3ErWQgR5WrlcihtnJ0tHXUeOrO8ZV/R4O03QK0dqq6mm4lyiPSMQH
     * +FJDOvTKVTUssKZqwJz58oHhEmrARdlns87/I6KJClTUFLkqqNfs+avNJVgyeY+Q
     * W5g5xAgGwax/Dj0ApQIDAQABo1QwUjAOBgNVHQ8BAf8EBAMCAYYwDwYDVR0TAQH/
     * BAUwAwEB/zAdBgNVHQ4EFgQUCctZf4aycI8awznjwNnpv7tNsiMwEAYJKwYBBAGC
     * NxUBBAMCAQAwDQYJKoZIhvcNAQEMBQADggIBAKyvPl3CEZaJjqPnktaXFbgToqZC
     * LgLNFgVZJ8og6Lq46BrsTaiXVq5lQ7GPAJtSzVXNUzltYkyLDVt8LkS/gxCP81OC
     * gMNPOsduET/m4xaRhPtthH80dK2Jp86519efhGSSvpWhrQlTM93uCupKUY5vVau6
     * tZRGrox/2KJQJWVggEbbMwSubLWYdFQl3JPk+ONVFT24bcMKpBLBaYVu32TxU5nh
     * SnUgnZUP5NbcA/FZGOhHibJXWpS2qdgXKxdJ5XbLwVaZOjex/2kskZGT4d9Mozd2
     * TaGf+G0eHdP67Pv0RR0Tbc/3WeUiJ3IrhvNXuzDtJE3cfVa7o7P4NHmJweDyAmH3
     * pvwPuxwXC65B2Xy9J6P9LjrRk5Sxcx0ki69bIImtt2dmefU6xqaWM/5TkshGsRGR
     * xpl/j8nWZjEgQRCHLQzWwa80mMpkg/sTV9HB8Dx6jKXB/ZUhoHHBk2dxEuqPiApp
     * GWSZI1b7rCoucL5mxAyE7+WL85MB+GqQk2dLsmijtWKP6T+MejteD+eMuMZ87zf9
     * dOLITzNy4ZQ5bb0Sr74MTnB8G2+NszKTc0QWbej09+CVgI+WXTik9KveCjCHk9hN
     * AHFiRSdLOkKEW39lt2c0Ui2cFmuqqNh7o0JMcccMyj6D5KbvtwEwXlGjefVwaaZB
     * RA+GsCyRxj3qrg+E
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x1e0\x1c\x06\x03U\x04\n\x13\x15Microsoft Corporation1604\x06\x03U\x04\x03\x13-Microsoft RSA Root Certificate Authority 2017",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xca[\xbe\x943\x8c)\x95\x91\x16\n\x95\xbdGb\xc1\x89\xf3\x996\xdfF\x90\xc9\xa5\xedxjoG\x91h\xf8\'gP3\x1d\xa1\xa6\xfb\xe0\xe5C\xa3\x84\x02W\x01]\x9cH@\x82S\x10\xbc\xbf\xc7;h\x90\xb6\x82-\xe5\xf4e\xd0\xccm\x19\xcc\x95\xf9{\xacJ\x94\xad\x0e\xdeKC\x1d\x87\x07\x92\x13\x90\x80\x83d59\x04\xfc\xe5\xe9l\xb3\xb6\x1fP\x948eP\\\x17F\xb9\xb6\x85\xb5\x1c\xb5\x17\xe8\xd6E\x9d\xd8\xb2&\xb0\xca\xc4pJ\xae`\xa4\xdd\xb3\xd9\xec\xfc;\xd5Wr\xbc?\xc8\xc9\xb2\xdeKk\xf8#l\x03\xc0\x05\xbd\x95\xc7\xcds;f\x80d\xe3\x1a\xac.\xf9G\x05\xf2\x06\xb6\x9bs\xf5x3[\xc7\xa1\xfb\'*\xa1\xb4\x9a\x91\x8c\x91\xd3:\x82>v@\xb4\xcdRaQp(?\xc5\xc5Z\xf2\xc9\x8cI\xbb\x14[M\xc8\xffgML\x12\x96\xad\xf5\xfex\xa8\x97\x87\xd7\xfd^ \x80\xdc\xa1K\"\xfb\xd4\x89\xad\xba\xceG\x97GU{\x8fE\xc8g(\x84\x95\x1ch0\xef\xefI\xe05{d\xe7\x98\xb0\x94\xdaM\x85;>U\xc4(\xafW\xf3\x9e\x13\xdbF\'\x9f\x1e\xa2^D\x83\xa4\xa5\xca\xd5\x13\xb3K?\xc4\xe3\xc2\xe6\x86a\xa4R0\xb9z Oo\x0f8S\xcb3\x0c\x13+\x8f\xd6\x9a\xbd*\xc8-\xb1\x1c}KQ\xcaG\xd1H\'r]\x87\xeb\xd5E\xe6He\x9d\xafR\x90\xba[\xa2\x18eW\x12\x9fh\xb9\xd4\x15k\x94\xc4i\"\x98\xf43\xe0\xed\xf9Q\x8eAP\xc94Ov\x90\xac\xfc8\xc1\xd8\xe1{\xb9\xe3\xe3\x94\xe1Fi\xcb\x0e\nPk\x13\xba\xac\x0f7Z\xb7\x12\xb5\x90\x81\x1eV\xaeW\"\x86\xd9\xc9\xd2\xd1\xd7Q\xe3\xab;\xc6U\xfd\x1e\x0e\xd3t\n\xd1\xda\xaa\xeai\xb8\x97(\x8fH\xc4\x07\xf8RC:\xf4\xcaU5,\xb0\xa6j\xc0\x9c\xf9\xf2\x81\xe1\x12j\xc0E\xd9g\xb3\xce\xff#\xa2\x89\nT\xd4\x14\xb9*\xa8\xd7\xec\xf9\xab\xcd%X2y\x8f\x90[\x989\xc4\x08\x06\xc1\xac\x7f\x0e=\x00\xa5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R2
     * Subject: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R2
     * Label: "GlobalSign Root CA - R2"
     * Serial: 4835703278459682885658125
     * MD5 Fingerprint: 94:14:77:7e:3e:5e:fd:8f:30:bd:41:b0:cf:e7:d0:30
     * SHA1 Fingerprint: 75:e0:ab:b6:13:85:12:27:1c:04:f8:5f:dd:de:38:e4:b7:24:2e:fe
     * SHA256 Fingerprint: ca:42:dd:41:74:5f:d0:b8:1e:b9:02:36:2c:f9:d8:bf:71:9d:a1:bd:1b:1e:fc:94:6f:5b:4c:99:f4:2c:1b:9e
     * -----BEGIN CERTIFICATE-----
     * MIIDujCCAqKgAwIBAgILBAAAAAABD4Ym5g0wDQYJKoZIhvcNAQEFBQAwTDEgMB4G
     * A1UECxMXR2xvYmFsU2lnbiBSb290IENBIC0gUjIxEzARBgNVBAoTCkdsb2JhbFNp
     * Z24xEzARBgNVBAMTCkdsb2JhbFNpZ24wHhcNMDYxMjE1MDgwMDAwWhcNMjExMjE1
     * MDgwMDAwWjBMMSAwHgYDVQQLExdHbG9iYWxTaWduIFJvb3QgQ0EgLSBSMjETMBEG
     * A1UEChMKR2xvYmFsU2lnbjETMBEGA1UEAxMKR2xvYmFsU2lnbjCCASIwDQYJKoZI
     * hvcNAQEBBQADggEPADCCAQoCggEBAKbPJA6+Lm8omUVCxKs+IVSbC9N/hHD6ErPL
     * v4dfxn+G07IwXNb9rfF73OX4YJYJkhD10FPe+3t+c4isUoh7SqbKSaZeqKeMWhG8
     * eoLrvozps6yWJQeXSpkqBy+0Hne/ig+1AnwblrjFuTosvNYSuetZfeLQBoZfXklq
     * tTleiDTsvHgMCJiEbKjNS7SgfQx5TfC4LcshytVsW33hoCmEofnTlEnLJGKRILzd
     * C9XZzPnqJworc5HGnRusyMvo4KD0L5CLTfuwNhv2GXqF4G3yYROIXJ/gkwpRl4pa
     * zq+r1feqCapgvdzZX99yqWATXgAByUr6P6TqBwMhAo6CygPCm48CAwEAAaOBnDCB
     * mTAOBgNVHQ8BAf8EBAMCAQYwDwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUm+IH
     * V2ccHsBqBt5ZtJot39wZhi4wNgYDVR0fBC8wLTAroCmgJ4YlaHR0cDovL2NybC5n
     * bG9iYWxzaWduLm5ldC9yb290LXIyLmNybDAfBgNVHSMEGDAWgBSb4gdXZxwewGoG
     * 3lm0mi3f3BmGLjANBgkqhkiG9w0BAQUFAAOCAQEAmYFThxxol4aR7OBKuEQLq4Gs
     * J0/WwbgcQ3izDJr86iw8bmEbTUsp9Z8FHSbBuOmDAGJFtqkIk7mpM0sYmsL4h4hO
     * 291xNBrBVNpGP+DTKqttVCL1OmLNIG+6KYnX3ZHu01yiPqFbQfXf5WRDLenVOavS
     * ot+3i9DAgBkcRcAtjOj4LaR0VknFBbVPFd5uRHg5h6h+u/N5GJG79G+dwfCMNYxd
     * AfvDbbnvRG15RjF+Cv6pgsH/76tuIMRQyV+dTZsXjAzlAcmgQWpzU/qlULRuJQ/7
     * TBj0/VLZjmmx6BEP3ojY+x1J96relc8geMJgEtslQIxq/H5COEBkEveegeGTLg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1 0\x1e\x06\x03U\x04\x0b\x13\x17GlobalSign Root CA - R21\x130\x11\x06\x03U\x04\n\x13\nGlobalSign1\x130\x11\x06\x03U\x04\x03\x13\nGlobalSign",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xa6\xcf$\x0e\xbe.o(\x99EB\xc4\xab>!T\x9b\x0b\xd3\x7f\x84p\xfa\x12\xb3\xcb\xbf\x87_\xc6\x7f\x86\xd3\xb20\\\xd6\xfd\xad\xf1{\xdc\xe5\xf8`\x96\t\x92\x10\xf5\xd0S\xde\xfb{~s\x88\xacR\x88{J\xa6\xcaI\xa6^\xa8\xa7\x8cZ\x11\xbcz\x82\xeb\xbe\x8c\xe9\xb3\xac\x96%\x07\x97J\x99*\x07/\xb4\x1ew\xbf\x8a\x0f\xb5\x02|\x1b\x96\xb8\xc5\xb9:,\xbc\xd6\x12\xb9\xebY}\xe2\xd0\x06\x86_^Ij\xb59^\x884\xec\xbcx\x0c\x08\x98\x84l\xa8\xcdK\xb4\xa0}\x0cyM\xf0\xb8-\xcb!\xca\xd5l[}\xe1\xa0)\x84\xa1\xf9\xd3\x94I\xcb$b\x91 \xbc\xdd\x0b\xd5\xd9\xcc\xf9\xea\'\n+s\x91\xc6\x9d\x1b\xac\xc8\xcb\xe8\xe0\xa0\xf4/\x90\x8bM\xfb\xb06\x1b\xf6\x19z\x85\xe0m\xf2a\x13\x88\\\x9f\xe0\x93\nQ\x97\x8aZ\xce\xaf\xab\xd5\xf7\xaa\t\xaa`\xbd\xdc\xd9_\xdfr\xa9`\x13^\x00\x01\xc9J\xfa?\xa4\xea\x07\x03!\x02\x8e\x82\xca\x03\xc2\x9b\x8f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=DigiCert Global Root G2 O=DigiCert Inc OU=www.digicert.com
     * Subject: CN=DigiCert Global Root G2 O=DigiCert Inc OU=www.digicert.com
     * Label: "DigiCert Global Root G2"
     * Serial: 4293743540046975378534879503202253541
     * MD5 Fingerprint: e4:a6:8a:c8:54:ac:52:42:46:0a:fd:72:48:1b:2a:44
     * SHA1 Fingerprint: df:3c:24:f9:bf:d6:66:76:1b:26:80:73:fe:06:d1:cc:8d:4f:82:a4
     * SHA256 Fingerprint: cb:3c:cb:b7:60:31:e5:e0:13:8f:8d:d3:9a:23:f9:de:47:ff:c3:5e:43:c1:14:4c:ea:27:d4:6a:5a:b1:cb:5f
     * -----BEGIN CERTIFICATE-----
     * MIIDjjCCAnagAwIBAgIQAzrx5qcRqaC7KGSxHQn65TANBgkqhkiG9w0BAQsFADBh
     * MQswCQYDVQQGEwJVUzEVMBMGA1UEChMMRGlnaUNlcnQgSW5jMRkwFwYDVQQLExB3
     * d3cuZGlnaWNlcnQuY29tMSAwHgYDVQQDExdEaWdpQ2VydCBHbG9iYWwgUm9vdCBH
     * MjAeFw0xMzA4MDExMjAwMDBaFw0zODAxMTUxMjAwMDBaMGExCzAJBgNVBAYTAlVT
     * MRUwEwYDVQQKEwxEaWdpQ2VydCBJbmMxGTAXBgNVBAsTEHd3dy5kaWdpY2VydC5j
     * b20xIDAeBgNVBAMTF0RpZ2lDZXJ0IEdsb2JhbCBSb290IEcyMIIBIjANBgkqhkiG
     * 9w0BAQEFAAOCAQ8AMIIBCgKCAQEAuzfNNNx7a8myaJCtSnX/RrohCgiN9RlUyfuI
     * 2/Ou8jqJkTx65qsGGmvPrC3oXgkkRLpimn7Wo6h+4FR1IAWsULecYxpsMNzaHxmx
     * 1x7e/dfgy5SDN67sH0NO3Xss0r0upS/kqbitOtSZpLYl6ZtrAGCSYP9PIUkY92eQ
     * q2EGnI/yuum06ZIya7XzV+hdG82MHauVBJVJ8zUtluNJbd134/tJS7SsVQepj5Wz
     * tCO7TG1F8PapspUwtP1MVYwnSlcUfIKdzXOS0xZKBgyMUNGPHgm+F6HmIcr9g+UQ
     * vIOlCsRnKPZzFBQ9RnbDhxSJITRNrw9FDKZJobq7nMWxM4MphQIDAQABo0IwQDAP
     * BgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBhjAdBgNVHQ4EFgQUTiJUIBiV
     * 5uNu5g/6+rkS7QYXjzkwDQYJKoZIhvcNAQELBQADggEBAGBnKJRvDkhj6zHd6mcY
     * 1Yl9PMWLSn/pvtsrF9+wX3N3KjITOYFnQoQj8kVnNeyIv/iPsGEMNKSuIEyExtv4
     * NeF22d+mQrvHRAiGfzZ0JFrabA0UWTW98kndth/Jsw1HKj2ZL7tcu7XUIOGZX1NG
     * Fdtom/DzMNU+MeKNhJ7jitralj41E6Vf8PlwUHBHQRFXGU7Aj64GxJUTFy8bJZ91
     * 8rGOmaFvE7FBcf6IKshPECBV1/MUReXgRPTqh5Uykw7+U0b6LJ3/iyK5S9kJRaTe
     * pLiaWN0bfVKfjllDiIGknibVb63dDcY3fe0Dkhvld1927jyNxF1WW6LZZm6zNTfl
     * MrY=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x150\x13\x06\x03U\x04\n\x13\x0cDigiCert Inc1\x190\x17\x06\x03U\x04\x0b\x13\x10www.digicert.com1 0\x1e\x06\x03U\x04\x03\x13\x17DigiCert Global Root G2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbb7\xcd4\xdc{k\xc9\xb2h\x90\xadJu\xffF\xba!\n\x08\x8d\xf5\x19T\xc9\xfb\x88\xdb\xf3\xae\xf2:\x89\x91<z\xe6\xab\x06\x1ak\xcf\xac-\xe8^\t$D\xbab\x9a~\xd6\xa3\xa8~\xe0Tu \x05\xacP\xb7\x9cc\x1al0\xdc\xda\x1f\x19\xb1\xd7\x1e\xde\xfd\xd7\xe0\xcb\x94\x837\xae\xec\x1fCN\xdd{,\xd2\xbd.\xa5/\xe4\xa9\xb8\xad:\xd4\x99\xa4\xb6%\xe9\x9bk\x00`\x92`\xffO!I\x18\xf7g\x90\xaba\x06\x9c\x8f\xf2\xba\xe9\xb4\xe9\x922k\xb5\xf3W\xe8]\x1b\xcd\x8c\x1d\xab\x95\x04\x95I\xf35-\x96\xe3Im\xddw\xe3\xfbIK\xb4\xacU\x07\xa9\x8f\x95\xb3\xb4#\xbbLmE\xf0\xf6\xa9\xb2\x950\xb4\xfdLU\x8c\'JW\x14|\x82\x9d\xcds\x92\xd3\x16J\x06\x0c\x8cP\xd1\x8f\x1e\t\xbe\x17\xa1\xe6!\xca\xfd\x83\xe5\x10\xbc\x83\xa5\n\xc4g(\xf6s\x14\x14=Fv\xc3\x87\x14\x89!4M\xaf\x0fE\x0c\xa6I\xa1\xba\xbb\x9c\xc5\xb13\x83)\x85\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R3
     * Subject: CN=GlobalSign O=GlobalSign OU=GlobalSign Root CA - R3
     * Label: "GlobalSign Root CA - R3"
     * Serial: 4835703278459759426209954
     * MD5 Fingerprint: c5:df:b8:49:ca:05:13:55:ee:2d:ba:1a:c3:3e:b0:28
     * SHA1 Fingerprint: d6:9b:56:11:48:f0:1c:77:c5:45:78:c1:09:26:df:5b:85:69:76:ad
     * SHA256 Fingerprint: cb:b5:22:d7:b7:f1:27:ad:6a:01:13:86:5b:df:1c:d4:10:2e:7d:07:59:af:63:5a:7c:f4:72:0d:c9:63:c5:3b
     * -----BEGIN CERTIFICATE-----
     * MIIDXzCCAkegAwIBAgILBAAAAAABIVhTCKIwDQYJKoZIhvcNAQELBQAwTDEgMB4G
     * A1UECxMXR2xvYmFsU2lnbiBSb290IENBIC0gUjMxEzARBgNVBAoTCkdsb2JhbFNp
     * Z24xEzARBgNVBAMTCkdsb2JhbFNpZ24wHhcNMDkwMzE4MTAwMDAwWhcNMjkwMzE4
     * MTAwMDAwWjBMMSAwHgYDVQQLExdHbG9iYWxTaWduIFJvb3QgQ0EgLSBSMzETMBEG
     * A1UEChMKR2xvYmFsU2lnbjETMBEGA1UEAxMKR2xvYmFsU2lnbjCCASIwDQYJKoZI
     * hvcNAQEBBQADggEPADCCAQoCggEBAMwldpB5BngiFvXAg7aEyiie/QV2EcWtiHL8
     * RgJDx7KKnQRfJMsuS+FggkbhUqsMgUdwbN1k0ev1LKMPgj0MK66X17YUhhB5uzsT
     * gHeMCOFJ0mpiLx9e+pZo34knlTifBtc+ycsmWQ1z3rDI6SYOgxXG71uL0gRgykmm
     * KPZpO/bLyCiR5Z2KYVc3rHQU3HTgOu5yLy6c+9C7v/U9AOEGM+iCK65TpjoWc4zd
     * QQ4gOsC0p6Hpsk+QLjJg6VfLuQSSaGjlOCZgdbKfd/+RFO+uIEn8rUAVSNECMWEZ
     * XriX7613t2Saer9fwRPvm2L7DWzgVGkWqQPabumDk3F2xmmFghcCAwEAAaNCMEAw
     * DgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFI/wS3+o
     * LkUkrk1Q+mOai97i3Ru8MA0GCSqGSIb3DQEBCwUAA4IBAQBLQNvAUKr+yAzv95ZU
     * RUm7lgAJQayzE4aGKAczymvmdLm6AC2upArT9fHxD4q/c2dKg8dEe3jgr25sbwMp
     * jjM5RcOO5LlXbKr8EpbsU8Yt5CRsuZRj+9xTaGdWPoO4zzUhw8lo/s7awlOqzJCK
     * 6fBdRoyV3XpYKBovHd7NADdBj+1EbddTKJd+82cEHhXXipa0095MJ6RMG3NzdvQX
     * mcIfeg7jLQitChws/zyrVQ4PkX4268NXSb7hLi18YIvDQVETI53O9zJrlAGomecs
     * Mx86OyXShkDOOyyGeMlhLxS67ttVb9+E7gUJTb0o2HLO02JQZR7rkpeDMdmztcpH
     * WD9f
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1 0\x1e\x06\x03U\x04\x0b\x13\x17GlobalSign Root CA - R31\x130\x11\x06\x03U\x04\n\x13\nGlobalSign1\x130\x11\x06\x03U\x04\x03\x13\nGlobalSign",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xcc%v\x90y\x06x\"\x16\xf5\xc0\x83\xb6\x84\xca(\x9e\xfd\x05v\x11\xc5\xad\x88r\xfcF\x02C\xc7\xb2\x8a\x9d\x04_$\xcb.K\xe1`\x82F\xe1R\xab\x0c\x81Gpl\xddd\xd1\xeb\xf5,\xa3\x0f\x82=\x0c+\xae\x97\xd7\xb6\x14\x86\x10y\xbb;\x13\x80w\x8c\x08\xe1I\xd2jb/\x1f^\xfa\x96h\xdf\x89\'\x958\x9f\x06\xd7>\xc9\xcb&Y\rs\xde\xb0\xc8\xe9&\x0e\x83\x15\xc6\xef[\x8b\xd2\x04`\xcaI\xa6(\xf6i;\xf6\xcb\xc8(\x91\xe5\x9d\x8aaW7\xact\x14\xdct\xe0:\xeer/.\x9c\xfb\xd0\xbb\xbf\xf5=\x00\xe1\x063\xe8\x82+\xaeS\xa6:\x16s\x8c\xddA\x0e :\xc0\xb4\xa7\xa1\xe9\xb2O\x90.2`\xe9W\xcb\xb9\x04\x92hh\xe58&`u\xb2\x9fw\xff\x91\x14\xef\xae I\xfc\xad@\x15H\xd1\x021a\x19^\xb8\x97\xef\xadw\xb7d\x9az\xbf_\xc1\x13\xef\x9bb\xfb\rl\xe0Ti\x16\xa9\x03\xdan\xe9\x83\x93qv\xc6i\x85\x82\x17\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign Root E46 O=GlobalSign nv-sa
     * Subject: CN=GlobalSign Root E46 O=GlobalSign nv-sa
     * Label: "GlobalSign Root E46"
     * Serial: 1552617690338932563915843282459653771421763
     * MD5 Fingerprint: b5:b8:66:ed:de:08:83:e3:c9:e2:01:34:06:ac:51:6f
     * SHA1 Fingerprint: 39:b4:6c:d5:fe:80:06:eb:e2:2f:4a:bb:08:33:a0:af:db:b9:dd:84
     * SHA256 Fingerprint: cb:b9:c4:4d:84:b8:04:3e:10:50:ea:31:a6:9f:51:49:55:d7:bf:d2:e2:c6:b4:93:01:01:9a:d6:1d:9f:50:58
     * -----BEGIN CERTIFICATE-----
     * MIICCzCCAZGgAwIBAgISEdK7ujNu1LzmJGjFDYQdmOhDMAoGCCqGSM49BAMDMEYx
     * CzAJBgNVBAYTAkJFMRkwFwYDVQQKExBHbG9iYWxTaWduIG52LXNhMRwwGgYDVQQD
     * ExNHbG9iYWxTaWduIFJvb3QgRTQ2MB4XDTE5MDMyMDAwMDAwMFoXDTQ2MDMyMDAw
     * MDAwMFowRjELMAkGA1UEBhMCQkUxGTAXBgNVBAoTEEdsb2JhbFNpZ24gbnYtc2Ex
     * HDAaBgNVBAMTE0dsb2JhbFNpZ24gUm9vdCBFNDYwdjAQBgcqhkjOPQIBBgUrgQQA
     * IgNiAAScDrHPt+ieUnd1NPqlRqetMhkytAepJ8qUuwzSChDH2omwlwxwEwkBjtjq
     * R+q+soArzfwoDdusvKSGN+1wCAB16pMLey5SnCNoIwZD7JIvU4Tb+0cUB+hflGdd
     * yXqBPCCjQjBAMA4GA1UdDwEB/wQEAwIBhjAPBgNVHRMBAf8EBTADAQH/MB0GA1Ud
     * DgQWBBQxCpCPtsad0kRLgLWi5h+xEk8blTAKBggqhkjOPQQDAwNoADBlAjEA31SQ
     * 7Zvvi5QCkxeCmb6zniz2C5GMn0oUsfZkvLtoURMMA/cVi4RguYv/Uo7njLwcAjA8
     * +RHUjE7AwWHCFUyqqx0LMV87HOIAl0Qx5v5zli/altP+CAezNIm8BZ/3Hobui3A=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BE1\x190\x17\x06\x03U\x04\n\x13\x10GlobalSign nv-sa1\x1c0\x1a\x06\x03U\x04\x03\x13\x13GlobalSign Root E46",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\x9c\x0e\xb1\xcf\xb7\xe8\x9eRwu4\xfa\xa5F\xa7\xad2\x192\xb4\x07\xa9\'\xca\x94\xbb\x0c\xd2\n\x10\xc7\xda\x89\xb0\x97\x0cp\x13\t\x01\x8e\xd8\xeaG\xea\xbe\xb2\x80+\xcd\xfc(\r\xdb\xac\xbc\xa4\x867\xedp\x08\x00u\xea\x93\x0b{.R\x9c#h#\x06C\xec\x92/S\x84\xdb\xfbG\x14\x07\xe8_\x94g]\xc9z\x81< ",
        name_constraints: None
    },

    /*
     * Issuer: CN=XRamp Global Certification Authority O=XRamp Security Services Inc OU=www.xrampsecurity.com
     * Subject: CN=XRamp Global Certification Authority O=XRamp Security Services Inc OU=www.xrampsecurity.com
     * Label: "XRamp Global CA Root"
     * Serial: 107108908803651509692980124233745014957
     * MD5 Fingerprint: a1:0b:44:b3:ca:10:d8:00:6e:9d:0f:d8:0f:92:0a:d1
     * SHA1 Fingerprint: b8:01:86:d1:eb:9c:86:a5:41:04:cf:30:54:f3:4c:52:b7:e5:58:c6
     * SHA256 Fingerprint: ce:cd:dc:90:50:99:d8:da:df:c5:b1:d2:09:b7:37:cb:e2:c1:8c:fb:2c:10:c0:ff:0b:cf:0d:32:86:fc:1a:a2
     * -----BEGIN CERTIFICATE-----
     * MIIEMDCCAxigAwIBAgIQUJRs7Bjq1ZxN1ZfvdY+grTANBgkqhkiG9w0BAQUFADCB
     * gjELMAkGA1UEBhMCVVMxHjAcBgNVBAsTFXd3dy54cmFtcHNlY3VyaXR5LmNvbTEk
     * MCIGA1UEChMbWFJhbXAgU2VjdXJpdHkgU2VydmljZXMgSW5jMS0wKwYDVQQDEyRY
     * UmFtcCBHbG9iYWwgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMDQxMTAxMTcx
     * NDA0WhcNMzUwMTAxMDUzNzE5WjCBgjELMAkGA1UEBhMCVVMxHjAcBgNVBAsTFXd3
     * dy54cmFtcHNlY3VyaXR5LmNvbTEkMCIGA1UEChMbWFJhbXAgU2VjdXJpdHkgU2Vy
     * dmljZXMgSW5jMS0wKwYDVQQDEyRYUmFtcCBHbG9iYWwgQ2VydGlmaWNhdGlvbiBB
     * dXRob3JpdHkwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCYJB69FbS6
     * 38eMpSe2OAtp87ZOqCwuIR1cRN8hXX4jdP5efrRKt6atH67gBhbim1vZZ3RrXYCP
     * KZ2GG9mcDZhtdhAoWORlsH9KmHmf4MMxfoArtYzAQDsRhtDLooY2YKTVMIJt2W7Q
     * DxIEM5dfT2Fa8OT5kavnHTu86M/0ay00fOJIYRyO82FEzG+gSqmUsE3a56k0enI4
     * qEHMPJQRfevIpoy3hsvKMzvZPTeL+3o+hiznc9cKV6xkmxnr9A8ECIqsAxcZZPRa
     * JSKNNCyy9mgdEm3Tih4U2sSPpuIjhdV6Db1q4Ons7Be7QhtnqiXtRYMh/MHJfNVi
     * PvryxS3T/dRlAgMBAAGjgZ8wgZwwEwYJKwYBBAGCNxQCBAYeBABDAEEwCwYDVR0P
     * BAQDAgGGMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFMZPoj0GY4QJnM5i5ASs
     * jVy16bYbMDYGA1UdHwQvMC0wK6ApoCeGJWh0dHA6Ly9jcmwueHJhbXBzZWN1cml0
     * eS5jb20vWEdDQS5jcmwwEAYJKwYBBAGCNxUBBAMCAQEwDQYJKoZIhvcNAQEFBQAD
     * ggEBAJEVOQMBG2f7Shz5CmBbodpNl2L5JFMn14JkTpAuw0kbK5rc/Kh4ZzXxHfAR
     * vbdI4xD2Dd8/0sm2qlWkSLoC295ZLhVbO50WfUfXN+pfTXYSNrsf16GBBEYgoyxt
     * qZ4Bfj8pzgCT3/3JknOJiWSe5yvkHJEs0rnOfc5vMZnT5r7SHpDwCRR5XCOrTdLa
     * IR9NmXmd4c8nnxCbHIgNsIpkQTG4DmyQJKSbXHGPurt+HBvbaoAPIbzp26a3QPSy
     * i6mx5O+aGtA9aZnuqCij4Tyz8LIRnM98QObd50N9otg6tamN8jSZxNQQ4Qb9CYQQ
     * O+7ETPTsJ3xCwnR8gooJybQDJbw=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x1e0\x1c\x06\x03U\x04\x0b\x13\x15www.xrampsecurity.com1$0\"\x06\x03U\x04\n\x13\x1bXRamp Security Services Inc1-0+\x06\x03U\x04\x03\x13$XRamp Global Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\x98$\x1e\xbd\x15\xb4\xba\xdf\xc7\x8c\xa5\'\xb68\x0bi\xf3\xb6N\xa8,.!\x1d\\D\xdf!]~#t\xfe^~\xb4J\xb7\xa6\xad\x1f\xae\xe0\x06\x16\xe2\x9b[\xd9gtk]\x80\x8f)\x9d\x86\x1b\xd9\x9c\r\x98mv\x10(X\xe4e\xb0\x7fJ\x98y\x9f\xe0\xc31~\x80+\xb5\x8c\xc0@;\x11\x86\xd0\xcb\xa2\x866`\xa4\xd50\x82m\xd9n\xd0\x0f\x12\x043\x97_OaZ\xf0\xe4\xf9\x91\xab\xe7\x1d;\xbc\xe8\xcf\xf4k-4|\xe2Ha\x1c\x8e\xf3aD\xcco\xa0J\xa9\x94\xb0M\xda\xe7\xa94zr8\xa8A\xcc<\x94\x11}\xeb\xc8\xa6\x8c\xb7\x86\xcb\xca3;\xd9=7\x8b\xfbz>\x86,\xe7s\xd7\nW\xacd\x9b\x19\xeb\xf4\x0f\x04\x08\x8a\xac\x03\x17\x19d\xf4Z%\"\x8d4,\xb2\xf6h\x1d\x12m\xd3\x8a\x1e\x14\xda\xc4\x8f\xa6\xe2#\x85\xd5z\r\xbdj\xe0\xe9\xec\xec\x17\xbbB\x1bg\xaa%\xedE\x83!\xfc\xc1\xc9|\xd5b>\xfa\xf2\xc5-\xd3\xfd\xd4e\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TrustCor RootCert CA-1 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Subject: CN=TrustCor RootCert CA-1 O=TrustCor Systems S. de R.L. OU=TrustCor Certificate Authority
     * Label: "TrustCor RootCert CA-1"
     * Serial: 15752444095811006489
     * MD5 Fingerprint: 6e:85:f1:dc:1a:00:d3:22:d5:b2:b2:ac:6b:37:05:45
     * SHA1 Fingerprint: ff:bd:cd:e7:82:c8:43:5e:3c:6f:26:86:5c:ca:a8:3a:45:5b:c3:0a
     * SHA256 Fingerprint: d4:0e:9c:86:cd:8f:e4:68:c1:77:69:59:f4:9e:a7:74:fa:54:86:84:b6:c4:06:f3:90:92:61:f4:dc:e2:57:5c
     * -----BEGIN CERTIFICATE-----
     * MIIEMDCCAxigAwIBAgIJANqb7HHzA7AZMA0GCSqGSIb3DQEBCwUAMIGkMQswCQYD
     * VQQGEwJQQTEPMA0GA1UECAwGUGFuYW1hMRQwEgYDVQQHDAtQYW5hbWEgQ2l0eTEk
     * MCIGA1UECgwbVHJ1c3RDb3IgU3lzdGVtcyBTLiBkZSBSLkwuMScwJQYDVQQLDB5U
     * cnVzdENvciBDZXJ0aWZpY2F0ZSBBdXRob3JpdHkxHzAdBgNVBAMMFlRydXN0Q29y
     * IFJvb3RDZXJ0IENBLTEwHhcNMTYwMjA0MTIzMjE2WhcNMjkxMjMxMTcyMzE2WjCB
     * pDELMAkGA1UEBhMCUEExDzANBgNVBAgMBlBhbmFtYTEUMBIGA1UEBwwLUGFuYW1h
     * IENpdHkxJDAiBgNVBAoMG1RydXN0Q29yIFN5c3RlbXMgUy4gZGUgUi5MLjEnMCUG
     * A1UECwweVHJ1c3RDb3IgQ2VydGlmaWNhdGUgQXV0aG9yaXR5MR8wHQYDVQQDDBZU
     * cnVzdENvciBSb290Q2VydCBDQS0xMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIB
     * CgKCAQEAv463leLCJhJrMxnHQFgKq1mqjQCj/IDHUHuO1CAmujIS2CNUSSUQIpid
     * RtLByZ5OGy4sDjjzGiVoHKZaBeYei0i/mJZ0PmnK6bV4pQa81QBeCQryJ3pS/C3V
     * seq0iWEk8xoT26nPUu0MJLq5nux+AHT6k61sKZKuUbS701e/s/OojZz0JEsq1pme
     * 9J7+wH5COucLlVPat2gOkEz7cD+PSiyU8ybdY2mplNgQTsVHCJCZGxdNuWxu72CV
     * EY4hgLW9oHPY0LJ3xEXqWib7ZnZ2+AYfYW0PVcWDtxBWcgYHpfOxGgMFZA6dWorW
     * hnAbJN7+KIor0Gqw/Hqi3LJ5DotlDwIDAQABo2MwYTAdBgNVHQ4EFgQU7mtJPHo/
     * DeOxCbeKyKsZn3MzUOcwHwYDVR0jBBgwFoAU7mtJPHo/DeOxCbeKyKsZn3MzUOcw
     * DwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAYYwDQYJKoZIhvcNAQELBQAD
     * ggEBACUY1JGPE+6PHh0RU9otRCkZoB5rMZ5NDp6tPVxBb5UrJKF5mDo4Nvu7Zp5I
     * /5CQ7z3UuJu0h3U/IJvOcs+hVcFNZKIZBqEHMwwLKeXx6quj7LUKdJDHfXLy11yf
     * ke+Ri7fc7Waiz45mO7yfOgLgJ90WmMCV1Aqk5IGadZQ1nJBfiDcGrVmVCrDRZ9MZ
     * yonnMlo2HD6CqFqTvsbQZJG2z9m2GM/bftJlo6bEjhcxwft+dtvTheNYsnd6djts
     * L1Ac59v2Z3kf9YKVmgenFK+P3CghZwnS1k1aHBkcjndcw5QkPTJrS37UeJSDvjdN
     * zl/HHk484IkzlQsPpTLWPFp5LBk=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02PA1\x0f0\r\x06\x03U\x04\x08\x0c\x06Panama1\x140\x12\x06\x03U\x04\x07\x0c\x0bPanama City1$0\"\x06\x03U\x04\n\x0c\x1bTrustCor Systems S. de R.L.1\'0%\x06\x03U\x04\x0b\x0c\x1eTrustCor Certificate Authority1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16TrustCor RootCert CA-1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbf\x8e\xb7\x95\xe2\xc2&\x12k3\x19\xc7@X\n\xabY\xaa\x8d\x00\xa3\xfc\x80\xc7P{\x8e\xd4 &\xba2\x12\xd8#TI%\x10\"\x98\x9dF\xd2\xc1\xc9\x9eN\x1b.,\x0e8\xf3\x1a%h\x1c\xa6Z\x05\xe6\x1e\x8bH\xbf\x98\x96t>i\xca\xe9\xb5x\xa5\x06\xbc\xd5\x00^\t\n\xf2\'zR\xfc-\xd5\xb1\xea\xb4\x89a$\xf3\x1a\x13\xdb\xa9\xcfR\xed\x0c$\xba\xb9\x9e\xec~\x00t\xfa\x93\xadl)\x92\xaeQ\xb4\xbb\xd3W\xbf\xb3\xf3\xa8\x8d\x9c\xf4$K*\xd6\x99\x9e\xf4\x9e\xfe\xc0~B:\xe7\x0b\x95S\xda\xb7h\x0e\x90L\xfbp?\x8fJ,\x94\xf3&\xddci\xa9\x94\xd8\x10N\xc5G\x08\x90\x99\x1b\x17M\xb9ln\xef`\x95\x11\x8e!\x80\xb5\xbd\xa0s\xd8\xd0\xb2w\xc4E\xeaZ&\xfbfvv\xf8\x06\x1fam\x0fU\xc5\x83\xb7\x10Vr\x06\x07\xa5\xf3\xb1\x1a\x03\x05d\x0e\x9dZ\x8a\xd6\x86p\x1b$\xde\xfe(\x8a+\xd0j\xb0\xfcz\xa2\xdc\xb2y\x0e\x8be\x0f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=UCA Extended Validation Root O=UniTrust
     * Subject: CN=UCA Extended Validation Root O=UniTrust
     * Label: "UCA Extended Validation Root"
     * Serial: 106100277556486529736699587978573607008
     * MD5 Fingerprint: a1:f3:5f:43:c6:34:9b:da:bf:8c:7e:05:53:ad:96:e2
     * SHA1 Fingerprint: a3:a1:b0:6f:24:61:23:4a:e3:36:a5:c2:37:fc:a6:ff:dd:f0:d7:3a
     * SHA256 Fingerprint: d4:3a:f9:b3:54:73:75:5c:96:84:fc:06:d7:d8:cb:70:ee:5c:28:e7:73:fb:29:4e:b4:1e:e7:17:22:92:4d:24
     * -----BEGIN CERTIFICATE-----
     * MIIFWjCCA0KgAwIBAgIQT9Irj/VkyDOeTzRYZiNwYDANBgkqhkiG9w0BAQsFADBH
     * MQswCQYDVQQGEwJDTjERMA8GA1UECgwIVW5pVHJ1c3QxJTAjBgNVBAMMHFVDQSBF
     * eHRlbmRlZCBWYWxpZGF0aW9uIFJvb3QwHhcNMTUwMzEzMDAwMDAwWhcNMzgxMjMx
     * MDAwMDAwWjBHMQswCQYDVQQGEwJDTjERMA8GA1UECgwIVW5pVHJ1c3QxJTAjBgNV
     * BAMMHFVDQSBFeHRlbmRlZCBWYWxpZGF0aW9uIFJvb3QwggIiMA0GCSqGSIb3DQEB
     * AQUAA4ICDwAwggIKAoICAQCpCQcoEwKwmeBkqh5DFnpzsZGgdT6o+uM4AHrsiWog
     * D4vFsJszA1qGxliG1cGFu0/GnEBNyr7uaZa4rYEwmnySBesFK5pI0Lh2PpbIILvS
     * sPGP2KxFRv+qZ2C0d35qHzwaUnoEPQc8hQ2E0B92CvdqFN9y4zR8V05WAT558aop
     * O2z6+I9tTcg1367r3CTueUWnhbYFiN6IXSV8l2RnCdm/WhUFhvMJHuxYMjMR83dk
     * sHYf5BA1FxvyDrFspCqjc/wJHx4yGVMR59mzLC52LqGj3n5qiAno8geK+LLNEOfi
     * c0CTuwjRP+H8C5SzJe98ptfRr5//lpr1kXuYC3fUfugH0mK1lTnj8/FtDw5lhIpj
     * VMWAtuCeS31HJqcBCF3RiJ7XwzJE+oJKCmhUfzhTA8ykADNkUVkLo4KRel7sFsLz
     * KuZi2irbWWIQJUoqgQtHB0MGcIfS+pMRKXpITeuUx3BNr2fVUbGAIAEBtHoIppB/
     * TuDvB0GHr2qlXov7z1CymlSvw4m6WC31MJixNnI5fkkE/SmnTHnkBVfblLkWU41G
     * sx2VYVdWf6/wFlthWG82UBEL2KwrlRYaDh8IzTY0ZRBiZtWAXxQgXy0MoHgKaNYs
     * 1+lvK9JKBZP8nm9rZ/+I8U6laUpSNwXqxhaN0sSZ0YIrO7o1dfdRUVjzyAfd5LQD
     * fwIDAQABo0IwQDAdBgNVHQ4EFgQU2XQ65DA9DfcS3H5aBZ8eNJr34RQwDwYDVR0T
     * AQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAYYwDQYJKoZIhvcNAQELBQADggIBADaN
     * l8xCFWQpN5smLNb7rhVpLGsaGvdftvkHTFnq88nIua7Mui563MD1sC3AO6+fcAUR
     * ap8lTwEpcOPlDOHqWnzcSbvBHiqB9RZLcpHIojG5qtr8nR/zXUACE/xOHAbKsxSQ
     * VBcZEhrxH9cMaVr2cXj0lH2RC47skFSOvG+hTKv8dGT9cZr4QQehzZHkPJrgmzI5
     * c6sq1WnIeJEmMX3ixzDx/BR4dxIOE/TdFpS/S2d7cFOFyrC78zhNLJA5wA3CXWvp
     * 4uXViI3WLL+rG761KIcSF3Ru/H38j9CHJrAb+7lsq+KePRXBOy5nAliRn+/4Qh8s
     * t2j1da3Ptfb/EX3C8CSlrdP6oDyp+l3cpaDvRKS+1ujl5BOWF3sGPjLtx7dCvHaj
     * 2GU4Kzg1USEODm8uNBNA4StnDG1KQTAYI1oyVZnJF+A83vbsea0rWBmirSwiGpWO
     * vpaQXUJXxPkUAzUrHC1RVwinOt4/5Mi0A3PCwSaAuwtCH60NryZy2sy+s6ODWA2C
     * xR9GUeOcGMyNm43sSet1UNWMKFnKdDTajAshqx7qG+XH/RU+wBeq+yNuJkbL+vmx
     * cmtpzyKEC2IPrNkZAJSidjzULZrtBJ4tBmIQN1IchXIbJ+XMxjHsN+xjWZsLHXbM
     * fjKaiJUINlK73nZfdklJrX+9ZSCyycErdhh2n1ax
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02CN1\x110\x0f\x06\x03U\x04\n\x0c\x08UniTrust1%0#\x06\x03U\x04\x03\x0c\x1cUCA Extended Validation Root",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa9\t\x07(\x13\x02\xb0\x99\xe0d\xaa\x1eC\x16zs\xb1\x91\xa0u>\xa8\xfa\xe38\x00z\xec\x89j \x0f\x8b\xc5\xb0\x9b3\x03Z\x86\xc6X\x86\xd5\xc1\x85\xbbO\xc6\x9c@M\xca\xbe\xeei\x96\xb8\xad\x810\x9a|\x92\x05\xeb\x05+\x9aH\xd0\xb8v>\x96\xc8 \xbb\xd2\xb0\xf1\x8f\xd8\xacEF\xff\xaag`\xb4w~j\x1f<\x1aRz\x04=\x07<\x85\r\x84\xd0\x1fv\n\xf7j\x14\xdfr\xe34|WNV\x01>y\xf1\xaa);l\xfa\xf8\x8fmM\xc85\xdf\xae\xeb\xdc$\xeeyE\xa7\x85\xb6\x05\x88\xde\x88]%|\x97dg\t\xd9\xbfZ\x15\x05\x86\xf3\t\x1e\xecX23\x11\xf3wd\xb0v\x1f\xe4\x105\x17\x1b\xf2\x0e\xb1l\xa4*\xa3s\xfc\t\x1f\x1e2\x19S\x11\xe7\xd9\xb3,.v.\xa1\xa3\xde~j\x88\t\xe8\xf2\x07\x8a\xf8\xb2\xcd\x10\xe7\xe2s@\x93\xbb\x08\xd1?\xe1\xfc\x0b\x94\xb3%\xef|\xa6\xd7\xd1\xaf\x9f\xff\x96\x9a\xf5\x91{\x98\x0bw\xd4~\xe8\x07\xd2b\xb5\x959\xe3\xf3\xf1m\x0f\x0ee\x84\x8acT\xc5\x80\xb6\xe0\x9eK}G&\xa7\x01\x08]\xd1\x88\x9e\xd7\xc32D\xfa\x82J\nhT\x7f8S\x03\xcc\xa4\x003dQY\x0b\xa3\x82\x91z^\xec\x16\xc2\xf3*\xe6b\xda*\xdbYb\x10%J*\x81\x0bG\x07C\x06p\x87\xd2\xfa\x93\x11)zHM\xeb\x94\xc7pM\xafg\xd5Q\xb1\x80 \x01\x01\xb4z\x08\xa6\x90\x7fN\xe0\xef\x07A\x87\xafj\xa5^\x8b\xfb\xcfP\xb2\x9aT\xaf\xc3\x89\xbaX-\xf50\x98\xb16r9~I\x04\xfd)\xa7Ly\xe4\x05W\xdb\x94\xb9\x16S\x8dF\xb3\x1d\x95aWV\x7f\xaf\xf0\x16[aXo6P\x11\x0b\xd8\xac+\x95\x16\x1a\x0e\x1f\x08\xcd64e\x10bf\xd5\x80_\x14 _-\x0c\xa0x\nh\xd6,\xd7\xe9o+\xd2J\x05\x93\xfc\x9eokg\xff\x88\xf1N\xa5iJR7\x05\xea\xc6\x16\x8d\xd2\xc4\x99\xd1\x82+;\xba5u\xf7QQX\xf3\xc8\x07\xdd\xe4\xb4\x03\x7f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Certigna Root CA O=Dhimyotis OU=0002 48146308100036
     * Subject: CN=Certigna Root CA O=Dhimyotis OU=0002 48146308100036
     * Label: "Certigna Root CA"
     * Serial: 269714418870597844693661054334862075617
     * MD5 Fingerprint: 0e:5c:30:62:27:eb:5b:bc:d7:ae:62:ba:e9:d5:df:77
     * SHA1 Fingerprint: 2d:0d:52:14:ff:9e:ad:99:24:01:74:20:47:6e:6c:85:27:27:f5:43
     * SHA256 Fingerprint: d4:8d:3d:23:ee:db:50:a4:59:e5:51:97:60:1c:27:77:4b:9d:7b:18:c9:4d:5a:05:95:11:a1:02:50:b9:31:68
     * -----BEGIN CERTIFICATE-----
     * MIIGWzCCBEOgAwIBAgIRAMrpG4nxVQMNo+ZBbcTjpuEwDQYJKoZIhvcNAQELBQAw
     * WjELMAkGA1UEBhMCRlIxEjAQBgNVBAoMCURoaW15b3RpczEcMBoGA1UECwwTMDAw
     * MiA0ODE0NjMwODEwMDAzNjEZMBcGA1UEAwwQQ2VydGlnbmEgUm9vdCBDQTAeFw0x
     * MzEwMDEwODMyMjdaFw0zMzEwMDEwODMyMjdaMFoxCzAJBgNVBAYTAkZSMRIwEAYD
     * VQQKDAlEaGlteW90aXMxHDAaBgNVBAsMEzAwMDIgNDgxNDYzMDgxMDAwMzYxGTAX
     * BgNVBAMMEENlcnRpZ25hIFJvb3QgQ0EwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAw
     * ggIKAoICAQDNGDllGlmx6mQWDoyUJJV8g9PFOSbcDO8WV43X2KyjQn+Cyu3NW9sO
     * ty3tRQgXstmzy9YXUnIo245Onoq2C/mehJpNdt4iKVzSs9IGPjA5qXSjklYcoW9M
     * CiBtnyN6tMbaLOQdLNyzKNAT8kxOAkmhVECe5uUFoC2EyP+YbNDrihqECB63aCPu
     * I9Vwzm1RaRDuoXrC0SIxwoKF0vJVdlB8JXrJhFwLrN1CTivngqIkicuQstDuI7pm
     * TLtipPlTWmR7fJj6o0ieD5Wupxj0auwuA0Wv8HT4Ks16XdG+RCYyKfHx9WzMfgIh
     * C59vpD++nVPiz32pLHxYGpfhPTc3GGYo0kDFUYqMwy3OU4gkWGQwFsWq4NYKpkDf
     * ePb1BHxpE4S80dGnBs8B92jAqFe7OmGtBIyT46388NtEbVncSVmurJqZNjBBe3Yz
     * IoejwpKGbvlw7q6Hh5UbxHq9MfPU0uWZ/75I7HX1eBYdpnDBfzwboZL7z8g81sWT
     * Co/1VTp2lc5ZmIoJlXcymoO6LAQ6l73UL77XbJuiyn1tJslV1c/DeVIICZkHJC1k
     * JWumIWmbat10TWuXekG9qxf5kBdIjzb5LdXF2+6qhUVB+s06RbFo5jZMm5BX7CO5
     * hwjCxAnxl4YqKE3idMDaxIzb3+KhF1nOJFl0Mdp//TBt2dzhauH8XwIDAQABo4IB
     * GjCCARYwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYE
     * FBiHVuBud+4kNTxOc5of1uHieX4rMB8GA1UdIwQYMBaAFBiHVuBud+4kNTxOc5of
     * 1uHieX4rMEQGA1UdIAQ9MDswOQYEVR0gADAxMC8GCCsGAQUFBwIBFiNodHRwczov
     * L3d3d3cuY2VydGlnbmEuZnIvYXV0b3JpdGVzLzBtBgNVHR8EZjBkMC+gLaArhilo
     * dHRwOi8vY3JsLmNlcnRpZ25hLmZyL2NlcnRpZ25hcm9vdGNhLmNybDAxoC+gLYYr
     * aHR0cDovL2NybC5kaGlteW90aXMuY29tL2NlcnRpZ25hcm9vdGNhLmNybDANBgkq
     * hkiG9w0BAQsFAAOCAgEAlLieT/DjlQgi581oQfccVdV8AOItOoldaDgvUSILSo3L
     * 6btdPrtcPbEo/uRTVRPPoZAbAh1fZkYJMyjhDSSXcNMQH+pkV5a7XdrnxIxPTGRG
     * HVyH41neQtGbqH6mid2PHMkwgu07nM3A6RngatgCdTer9zQoKJHyBApPNeNgJgH6
     * 0BGM+RFq7q89w1DTj18zeTyGqHNFkIwgtnJzFyO+B2XleJINugHA64wcZr+shncB
     * lA2c5uk5jR+mUYyZDDl34bSb+hxnV29qao6pK0xXeXpXIs/NX2NGjVxZOob4Mkdi
     * o2cNGJHc+6Zr9UhhcyNZjgKnvETq9Emd8VRY+WCv2hikLyhF3HqgiIZd8zvn/yk1
     * gPxkQ5Tm4xxvvq0OKmOZK8l+hfZx6AYDlf7ej0gcWtSS6Cvu5zHbugRqh5jnxV/v
     * faci9wHYTfmJ0A6aBVmknpjZbyvKcL5kwlWj9Omvw5Ip3IgWJJk8jSaYtlu3zM63
     * Nwf9JtmYhST/WSMDmu2dnajkXjjO11INb9I/bbEFa0nOipFGc/T2L/Coc3cOZayh
     * jWZSaX5LaAzHHjcng6WMxwLkFM1JAbBzs/3GkDpv0mztO+7skb6iQ12LAEpmJURw
     * 3kAP+HwV96LOPNdeE4yBFxgX0b3xdxA61GU5wSesVywlVP+i2k+KYTlerj1KjL0=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02FR1\x120\x10\x06\x03U\x04\n\x0c\tDhimyotis1\x1c0\x1a\x06\x03U\x04\x0b\x0c\x130002 481463081000361\x190\x17\x06\x03U\x04\x03\x0c\x10Certigna Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xcd\x189e\x1aY\xb1\xead\x16\x0e\x8c\x94$\x95|\x83\xd3\xc59&\xdc\x0c\xef\x16W\x8d\xd7\xd8\xac\xa3B\x7f\x82\xca\xed\xcd[\xdb\x0e\xb7-\xedE\x08\x17\xb2\xd9\xb3\xcb\xd6\x17Rr(\xdb\x8eN\x9e\x8a\xb6\x0b\xf9\x9e\x84\x9aMv\xde\")\\\xd2\xb3\xd2\x06>09\xa9t\xa3\x92V\x1c\xa1oL\n m\x9f#z\xb4\xc6\xda,\xe4\x1d,\xdc\xb3(\xd0\x13\xf2LN\x02I\xa1T@\x9e\xe6\xe5\x05\xa0-\x84\xc8\xff\x98l\xd0\xeb\x8a\x1a\x84\x08\x1e\xb7h#\xee#\xd5p\xcemQi\x10\xee\xa1z\xc2\xd1\"1\xc2\x82\x85\xd2\xf2UvP|%z\xc9\x84\\\x0b\xac\xddBN+\xe7\x82\xa2$\x89\xcb\x90\xb2\xd0\xee#\xbafL\xbbb\xa4\xf9SZd{|\x98\xfa\xa3H\x9e\x0f\x95\xae\xa7\x18\xf4j\xec.\x03E\xaf\xf0t\xf8*\xcdz]\xd1\xbeD&2)\xf1\xf1\xf5l\xcc~\x02!\x0b\x9fo\xa4?\xbe\x9dS\xe2\xcf}\xa9,|X\x1a\x97\xe1=77\x18f(\xd2@\xc5Q\x8a\x8c\xc3-\xceS\x88$Xd0\x16\xc5\xaa\xe0\xd6\n\xa6@\xdfx\xf6\xf5\x04|i\x13\x84\xbc\xd1\xd1\xa7\x06\xcf\x01\xf7h\xc0\xa8W\xbb:a\xad\x04\x8c\x93\xe3\xad\xfc\xf0\xdbDmY\xdcIY\xae\xac\x9a\x9960A{v3\"\x87\xa3\xc2\x92\x86n\xf9p\xee\xae\x87\x87\x95\x1b\xc4z\xbd1\xf3\xd4\xd2\xe5\x99\xff\xbeH\xecu\xf5x\x16\x1d\xa6p\xc1\x7f<\x1b\xa1\x92\xfb\xcf\xc8<\xd6\xc5\x93\n\x8f\xf5U:v\x95\xceY\x98\x8a\t\x95w2\x9a\x83\xba,\x04:\x97\xbd\xd4/\xbe\xd7l\x9b\xa2\xca}m&\xc9U\xd5\xcf\xc3yR\x08\t\x99\x07$-d%k\xa6!i\x9bj\xddtMk\x97zA\xbd\xab\x17\xf9\x90\x17H\x8f6\xf9-\xd5\xc5\xdb\xee\xaa\x85EA\xfa\xcd:E\xb1h\xe66L\x9b\x90W\xec#\xb9\x87\x08\xc2\xc4\t\xf1\x97\x86*(M\xe2t\xc0\xda\xc4\x8c\xdb\xdf\xe2\xa1\x17Y\xce$Yt1\xda\x7f\xfd0m\xd9\xdc\xe1j\xe1\xfc_\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=AAA Certificate Services O=Comodo CA Limited
     * Subject: CN=AAA Certificate Services O=Comodo CA Limited
     * Label: "Comodo AAA Services root"
     * Serial: 1
     * MD5 Fingerprint: 49:79:04:b0:eb:87:19:ac:47:b0:bc:11:51:9b:74:d0
     * SHA1 Fingerprint: d1:eb:23:a4:6d:17:d6:8f:d9:25:64:c2:f1:f1:60:17:64:d8:e3:49
     * SHA256 Fingerprint: d7:a7:a0:fb:5d:7e:27:31:d7:71:e9:48:4e:bc:de:f7:1d:5f:0c:3e:0a:29:48:78:2b:c8:3e:e0:ea:69:9e:f4
     * -----BEGIN CERTIFICATE-----
     * MIIEMjCCAxqgAwIBAgIBATANBgkqhkiG9w0BAQUFADB7MQswCQYDVQQGEwJHQjEb
     * MBkGA1UECAwSR3JlYXRlciBNYW5jaGVzdGVyMRAwDgYDVQQHDAdTYWxmb3JkMRow
     * GAYDVQQKDBFDb21vZG8gQ0EgTGltaXRlZDEhMB8GA1UEAwwYQUFBIENlcnRpZmlj
     * YXRlIFNlcnZpY2VzMB4XDTA0MDEwMTAwMDAwMFoXDTI4MTIzMTIzNTk1OVowezEL
     * MAkGA1UEBhMCR0IxGzAZBgNVBAgMEkdyZWF0ZXIgTWFuY2hlc3RlcjEQMA4GA1UE
     * BwwHU2FsZm9yZDEaMBgGA1UECgwRQ29tb2RvIENBIExpbWl0ZWQxITAfBgNVBAMM
     * GEFBQSBDZXJ0aWZpY2F0ZSBTZXJ2aWNlczCCASIwDQYJKoZIhvcNAQEBBQADggEP
     * ADCCAQoCggEBAL5AnfRu4ep2hxxNRUSOvkbIgwadwSr+GB+O5AL686tdUIoWMQua
     * BtDFcCLNSS1UY8y2bmhGC1Pqy0wkwLxyTurxFa70VJoSCsN6sjNg4tqJVfMiWPPe
     * 3M/vg4aijJRPn2jymJBGhCfHdr/jzDUsi14HZGWCwEiwqJH5YZ92IFCokcdmtet4
     * YgNW8IoaE+oxox6gmf049vYnMlhvB/VruPsUK6+3qszWY19zjNoFmag4qMsXeDZR
     * rOme9Hg6jc8P2ULimAyrL58OAd7vn5lJ8S3frHRNG5i1R8XlKdH5kBjHYpy+g8cm
     * ez6KJcfA3Z3mNWgQIJ2P2N7Sw4ScDV7oL8kCAwEAAaOBwDCBvTAdBgNVHQ4EFgQU
     * oBEKIz6W8Qfs4q8p74Klf9AwpLQwDgYDVR0PAQH/BAQDAgEGMA8GA1UdEwEB/wQF
     * MAMBAf8wewYDVR0fBHQwcjA4oDagNIYyaHR0cDovL2NybC5jb21vZG9jYS5jb20v
     * QUFBQ2VydGlmaWNhdGVTZXJ2aWNlcy5jcmwwNqA0oDKGMGh0dHA6Ly9jcmwuY29t
     * b2RvLm5ldC9BQUFDZXJ0aWZpY2F0ZVNlcnZpY2VzLmNybDANBgkqhkiG9w0BAQUF
     * AAOCAQEACFb8AvCb6P+k+tZ7xkSAzk/ExfYAWMymtrwUSWgEdujm7l3sAg9g1o1Q
     * GE8mTgHj5rCl7r+8dFRBv/38ErjHT1r0iWAFf2C3BUrz9vHCv8S5dIa2LX1rzNLz
     * Rt0vxuBqw8M0Ayx9lt1awg6nCpnBBYurDC/zXDrPbDdVCYfeU0BsWO/8tqtlbgT2
     * G9w84FoVxp7Z8VlIMCFlA2zs6SFz7JsDoeA3raAVGI/6ugLOpyypEBMs1OUIJqsi
     * l2D4kF501KKaU73yqWjgom7C12yxow+ev+to51byrvLjKzg6CYG1a4XXvi3tPxq3
     * smPi9WIsgtRqAEFQ8TmDn5XpNpaYbg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02GB1\x1b0\x19\x06\x03U\x04\x08\x0c\x12Greater Manchester1\x100\x0e\x06\x03U\x04\x07\x0c\x07Salford1\x1a0\x18\x06\x03U\x04\n\x0c\x11Comodo CA Limited1!0\x1f\x06\x03U\x04\x03\x0c\x18AAA Certificate Services",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbe@\x9d\xf4n\xe1\xeav\x87\x1cMED\x8e\xbeF\xc8\x83\x06\x9d\xc1*\xfe\x18\x1f\x8e\xe4\x02\xfa\xf3\xab]P\x8a\x161\x0b\x9a\x06\xd0\xc5p\"\xcdI-Tc\xcc\xb6nhF\x0bS\xea\xcbL$\xc0\xbcrN\xea\xf1\x15\xae\xf4T\x9a\x12\n\xc3z\xb23`\xe2\xda\x89U\xf3\"X\xf3\xde\xdc\xcf\xef\x83\x86\xa2\x8c\x94O\x9fh\xf2\x98\x90F\x84\'\xc7v\xbf\xe3\xcc5,\x8b^\x07de\x82\xc0H\xb0\xa8\x91\xf9a\x9fv P\xa8\x91\xc7f\xb5\xebxb\x03V\xf0\x8a\x1a\x13\xea1\xa3\x1e\xa0\x99\xfd8\xf6\xf6\'2Xo\x07\xf5k\xb8\xfb\x14+\xaf\xb7\xaa\xcc\xd6c_s\x8c\xda\x05\x99\xa88\xa8\xcb\x17x6Q\xac\xe9\x9e\xf4x:\x8d\xcf\x0f\xd9B\xe2\x98\x0c\xab/\x9f\x0e\x01\xde\xef\x9f\x99I\xf1-\xdf\xactM\x1b\x98\xb5G\xc5\xe5)\xd1\xf9\x90\x18\xc7b\x9c\xbe\x83\xc7&{>\x8a%\xc7\xc0\xdd\x9d\xe65h\x10 \x9d\x8f\xd8\xde\xd2\xc3\x84\x9c\r^\xe8/\xc9\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Entrust Root Certification Authority - G4 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2015 Entrust, Inc. - for authorized use only
     * Subject: CN=Entrust Root Certification Authority - G4 O=Entrust, Inc. OU=See www.entrust.net/legal-terms/(c) 2015 Entrust, Inc. - for authorized use only
     * Label: "Entrust Root Certification Authority - G4"
     * Serial: 289383649854506086828220374796556676440
     * MD5 Fingerprint: 89:53:f1:83:23:b7:7c:8e:05:f1:8c:71:38:4e:1f:88
     * SHA1 Fingerprint: 14:88:4e:86:26:37:b0:26:af:59:62:5c:40:77:ec:35:29:ba:96:01
     * SHA256 Fingerprint: db:35:17:d1:f6:73:2a:2d:5a:b9:7c:53:3e:c7:07:79:ee:32:70:a6:2f:b4:ac:42:38:37:24:60:e6:f0:1e:88
     * -----BEGIN CERTIFICATE-----
     * MIIGSzCCBDOgAwIBAgIRANm1Q3+vqTkPAAAAAFVlrVgwDQYJKoZIhvcNAQELBQAw
     * gb4xCzAJBgNVBAYTAlVTMRYwFAYDVQQKEw1FbnRydXN0LCBJbmMuMSgwJgYDVQQL
     * Ex9TZWUgd3d3LmVudHJ1c3QubmV0L2xlZ2FsLXRlcm1zMTkwNwYDVQQLEzAoYykg
     * MjAxNSBFbnRydXN0LCBJbmMuIC0gZm9yIGF1dGhvcml6ZWQgdXNlIG9ubHkxMjAw
     * BgNVBAMTKUVudHJ1c3QgUm9vdCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eSAtIEc0
     * MB4XDTE1MDUyNzExMTExNloXDTM3MTIyNzExNDExNlowgb4xCzAJBgNVBAYTAlVT
     * MRYwFAYDVQQKEw1FbnRydXN0LCBJbmMuMSgwJgYDVQQLEx9TZWUgd3d3LmVudHJ1
     * c3QubmV0L2xlZ2FsLXRlcm1zMTkwNwYDVQQLEzAoYykgMjAxNSBFbnRydXN0LCBJ
     * bmMuIC0gZm9yIGF1dGhvcml6ZWQgdXNlIG9ubHkxMjAwBgNVBAMTKUVudHJ1c3Qg
     * Um9vdCBDZXJ0aWZpY2F0aW9uIEF1dGhvcml0eSAtIEc0MIICIjANBgkqhkiG9w0B
     * AQEFAAOCAg8AMIICCgKCAgEAsewsQu7i0TD/pZJH4i3DumSXbcr3DbVZwbPLqGgZ
     * 2K+EbTBwXX7zLtJTmeH+H17ZSK9dE43b/2MzTdMAArzE+NEGCJR5WIoV3imz/f3E
     * T+iq4qA7ec2/a0My3dl0ELn39GjUu9CH1apLiipvKgS1sqbHoHrmSKvS0VnM1n4j
     * 5pds8ELl3FFLFUHtSUrJ3hCX1nbB76W1NhSXNdh4IjVS70O92yfbYVaCNNzLiGAM
     * C1rlLAHGVK/XqsEQe9IFWrhAnoanw5CGAlZSCXqc0ieCU0plUmr1POeo8pyvi73T
     * DtTUXm6Hnmo9RR3RXRv06QqsYJn7ibT/mCzPfB3pAqoEmh643IhuJbNsZvc8kPNX
     * wbMv9W3y+8qh+CmdRouzavbmZwe+LGcKKh9asj5XxNMhIWNlUpEbsZmOeX7m640A
     * 2Vqq6nPopIICR5b+W45UYaPrL0swsIsjdXJ8ITzI9vF01Bx7owVV7rtNOzK+mndm
     * nqxpkCIHH2E6lr7lmk/MBTwoWdPBDFSoWWG9yHJM6Nyfh3+9nEg2XpWjDrk4JFX8
     * dWbrAuMINClKxuMrLzOg2qOGpRKX/YAr2hRC45K9PvJdXmd0LhyIRyk0X+IyqJwl
     * N4y6mACXi0mWHv0liqzc2thddG5msP9E36EYxr5ILzeUePiVSj9/E15dWf10hkNj
     * c0kCAwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYD
     * VR0OBBYEFJ84xFYjwznooHFs6FRM5Og6sb9nMA0GCSqGSIb3DQEBCwUAA4ICAQAS
     * 5UKme4sPDORGpbZgQIeMJX6tuGguW8ZAdjwD+MlZ9POrYs4QjbRaZIxowLByQzTS
     * Gwv2LFPSypBLhmb8qoMi9IsabyZIrHZ3CL/FmFz0Jomee8O5ZDIBf9PD3Vht7LGr
     * hFV0d4QEJ1JrhkzO3bll/9bGXp+aEJlLdWr+aumXIOTkdnrG0CSqkM0gkLpHZPt/
     * B7NTeLUKYvJzQ85BK4FqLoUWlFPUa19yIqtRLULVAJyZv967lDtX/Zr1hstWO1uI
     * AeV8KEsD+UmDfLJ/fOPtjqF/YFOOVZ1QNBIPt5d7bIdKROf1beyAN/BYGW5KaHbw
     * H5Lk6rWS02FREAutp9lfx1/cH6NcjKF+m7ee01ZvZl4HliDtC3T7Zk6LERXpgUl+
     * b7DUUH8i119lAg2m9IUe2K4GS0qn0jFmwvjO5QimpAKWRGhXxNUzzxkvFMSUHHuk
     * 2fCfDrGA4tGeEWSpiBE6doLlYsKA2KSD7ZPvfC+QsDJMlhVoSFLUmQjAJOgc47Ol
     * IQ6SwJAfzyBfyjs4x7dtOvPmRLgOMWuIjnDrnBdSqEGULoe256YSxXXfW8AKbnuk
     * 5F6G+TaU33fD6Q3AOfF5u0aOq0NZJ7cguyPpVkAh7DE9ZapD8j3fcEThuk0mEDuY
     * n/PIjhs4ViFqUZPTkcpG2om3PVODLAgfi49T3f+sHw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x160\x14\x06\x03U\x04\n\x13\rEntrust, Inc.1(0&\x06\x03U\x04\x0b\x13\x1fSee www.entrust.net/legal-terms1907\x06\x03U\x04\x0b\x130(c) 2015 Entrust, Inc. - for authorized use only1200\x06\x03U\x04\x03\x13)Entrust Root Certification Authority - G4",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xb1\xec,B\xee\xe2\xd10\xff\xa5\x92G\xe2-\xc3\xbad\x97m\xca\xf7\r\xb5Y\xc1\xb3\xcb\xa8h\x19\xd8\xaf\x84m0p]~\xf3.\xd2S\x99\xe1\xfe\x1f^\xd9H\xaf]\x13\x8d\xdb\xffc3M\xd3\x00\x02\xbc\xc4\xf8\xd1\x06\x08\x94yX\x8a\x15\xde)\xb3\xfd\xfd\xc4O\xe8\xaa\xe2\xa0;y\xcd\xbfkC2\xdd\xd9t\x10\xb9\xf7\xf4h\xd4\xbb\xd0\x87\xd5\xaaK\x8a*o*\x04\xb5\xb2\xa6\xc7\xa0z\xe6H\xab\xd2\xd1Y\xcc\xd6~#\xe6\x97l\xf0B\xe5\xdcQK\x15A\xedIJ\xc9\xde\x10\x97\xd6v\xc1\xef\xa5\xb56\x14\x975\xd8x\"5R\xefC\xbd\xdb\'\xdbaV\x824\xdc\xcb\x88`\x0c\x0bZ\xe5,\x01\xc6T\xaf\xd7\xaa\xc1\x10{\xd2\x05Z\xb8@\x9e\x86\xa7\xc3\x90\x86\x02VR\tz\x9c\xd2\'\x82SJeRj\xf5<\xe7\xa8\xf2\x9c\xaf\x8b\xbd\xd3\x0e\xd4\xd4^n\x87\x9ej=E\x1d\xd1]\x1b\xf4\xe9\n\xac`\x99\xfb\x89\xb4\xff\x98,\xcf|\x1d\xe9\x02\xaa\x04\x9a\x1e\xb8\xdc\x88n%\xb3lf\xf7<\x90\xf3W\xc1\xb3/\xf5m\xf2\xfb\xca\xa1\xf8)\x9dF\x8b\xb3j\xf6\xe6g\x07\xbe,g\n*\x1fZ\xb2>W\xc4\xd3!!ceR\x91\x1b\xb1\x99\x8ey~\xe6\xeb\x8d\x00\xd9Z\xaa\xeas\xe8\xa4\x82\x02G\x96\xfe[\x8eTa\xa3\xeb/K0\xb0\x8b#ur|!<\xc8\xf6\xf1t\xd4\x1c{\xa3\x05U\xee\xbbM;2\xbe\x9awf\x9e\xaci\x90\"\x07\x1fa:\x96\xbe\xe5\x9aO\xcc\x05<(Y\xd3\xc1\x0cT\xa8Ya\xbd\xc8rL\xe8\xdc\x9f\x87\x7f\xbd\x9cH6^\x95\xa3\x0e\xb98$U\xfcuf\xeb\x02\xe3\x084)J\xc6\xe3+/3\xa0\xda\xa3\x86\xa5\x12\x97\xfd\x80+\xda\x14B\xe3\x92\xbd>\xf2]^gt.\x1c\x88G)4_\xe22\xa8\x9c%7\x8c\xba\x98\x00\x97\x8bI\x96\x1e\xfd%\x8a\xac\xdc\xda\xd8]tnf\xb0\xffD\xdf\xa1\x18\xc6\xbeH/7\x94x\xf8\x95J?\x7f\x13^]Y\xfdt\x86CcsI\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=TeliaSonera Root CA v1 O=TeliaSonera
     * Subject: CN=TeliaSonera Root CA v1 O=TeliaSonera
     * Label: "TeliaSonera Root CA v1"
     * Serial: 199041966741090107964904287217786801558
     * MD5 Fingerprint: 37:41:49:1b:18:56:9a:26:f5:ad:c2:66:fb:40:a5:4c
     * SHA1 Fingerprint: 43:13:bb:96:f1:d5:86:9b:c1:4e:6a:92:f6:cf:f6:34:69:87:82:37
     * SHA256 Fingerprint: dd:69:36:fe:21:f8:f0:77:c1:23:a1:a5:21:c1:22:24:f7:22:55:b7:3e:03:a7:26:06:93:e8:a2:4b:0f:a3:89
     * -----BEGIN CERTIFICATE-----
     * MIIFODCCAyCgAwIBAgIRAJW+FqD3LkbxezmCcvqLzZYwDQYJKoZIhvcNAQEFBQAw
     * NzEUMBIGA1UECgwLVGVsaWFTb25lcmExHzAdBgNVBAMMFlRlbGlhU29uZXJhIFJv
     * b3QgQ0EgdjEwHhcNMDcxMDE4MTIwMDUwWhcNMzIxMDE4MTIwMDUwWjA3MRQwEgYD
     * VQQKDAtUZWxpYVNvbmVyYTEfMB0GA1UEAwwWVGVsaWFTb25lcmEgUm9vdCBDQSB2
     * MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAMK+6yfwIaPzaSZVfp3F
     * VRaRXP3vIb9TgHot0pGMYzHw7CTww6XScnwQbfQ3t+XmfHnqjLWCi65ItqwA3GV1
     * 7CpNX8GH9SBlK4GoRz6JI5UwFpB/6FcHSOcZrr9FZ7E3GwYq/t75rH2D+1665I+X
     * Z75Ljo1kB1c4VWk0Nj0TSO9P4tNmHqTPGrdeNjPUtAa9GAH9d4RQAEX1jF3oI7x+
     * /jXh7VB7qTCNGdMJjmhnXb88lxhTuylixcpecsHHltTbLaC0H2kD7OriUPEMPPCs
     * 81Mt8Bz17Ww5OXOAFshSsCPN4D7c3TxHoLs1iuKYaIu+5b9y7tL6pe0S7fyYGKkm
     * dtwoSxAgHNN/Fnct7W+A90m7UwW7XWjH1Mh1Fj+JWov3F0fUTPHSiXk+TT2YqGHe
     * Oh7S+F4D4MHJHIzTjU3TlTazN19jY5szFPAtJmtTfImMMsJu7D0hADnJoWjiUIMu
     * sDor8zagrC/kb2HCUQk5PotTubtn2txTuXZZNp1D5SDgPTJghSJRt8czu90VL6R4
     * pgd7gUY2BIbdeTXHlSw7sKMXNeVzH7RcWe/a6hBle3rQf5+ztCo3O3CLm1u5K7fs
     * slESl1MpWtTwEhDcTwK7EpIvYtQ/aUN8Ddb8WHUBiJ1YFkveupD/RwGJBmr2X7KQ
     * arMCpgKIv7NHfirZ1fpoeDVNAgMBAAGjPzA9MA8GA1UdEwEB/wQFMAMBAf8wCwYD
     * VR0PBAQDAgEGMB0GA1UdDgQWBBTwj1k4ALP1j5qWDNXr+nuqF+gTEjANBgkqhkiG
     * 9w0BAQUFAAOCAgEAvuRcYk4k9AwI//DTDGjkk0kiP0Qnb7tt3oNmzqjMDfz1mgbl
     * dxSR651Be5kqhOX//CHBXfDkH1e3damhXwIm/9fH907eT/j3HEbAek9ALCI18Bmx
     * 0GtnLLCo4MBANzX2hFxc469CeP6nyQ1Q6g2EdvZR74NTxnr/DlZJLo961gzmJ1Tj
     * TQpgcmLNkQfWpb/ImWvtxBnmq0wROMVvMeJuScg/doAmAyYp4Db29iBT4xdwNBed
     * Y2gea+zDTYa4EzAvXUYNR0PVG6pZDrlcjQZIrXSHX8f8MVRBE+LHIQ6e4B4N4cB7
     * Q4WQxYpYxmUKeFfyxiMPAdkgS94P+5KFdSpcc41teyWRyu5FrgZLAMzTsVlQ2jqI
     * OylDRl6XK1TOU2+NSueW+r9xDkKLfP0ooNBIytrEgUy7onOTJsjrDNYmiLbAJM+7
     * vVvrdX3pCI6GMyx5dwlppYn8s3CQh3aP0yK7Qs69cwsgJirQmz1wHiRszYd2qReW
     * t88NkvuOGKmYSdGe/mBEciG5Ge3C9THxOUiIkCR1VBatzvT4aRRkOfujuLpwQMcn
     * HL/EVlP6Y2XQ8xwOFvVrhlhNGNTkDY6lnVuR3HYkUD/GKvvZt5y11ubQ2egZixVx
     * SK236thZiNSQvxaz2emsWWFUyBy6ysHK4bkgTI86k4mloMy/0/Z1pHWWbVY=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x140\x12\x06\x03U\x04\n\x0c\x0bTeliaSonera1\x1f0\x1d\x06\x03U\x04\x03\x0c\x16TeliaSonera Root CA v1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xc2\xbe\xeb\'\xf0!\xa3\xf3i&U~\x9d\xc5U\x16\x91\\\xfd\xef!\xbfS\x80z-\xd2\x91\x8cc1\xf0\xec$\xf0\xc3\xa5\xd2r|\x10m\xf47\xb7\xe5\xe6|y\xea\x8c\xb5\x82\x8b\xaeH\xb6\xac\x00\xdceu\xec*M_\xc1\x87\xf5 e+\x81\xa8G>\x89#\x950\x16\x90\x7f\xe8W\x07H\xe7\x19\xae\xbfEg\xb17\x1b\x06*\xfe\xde\xf9\xac}\x83\xfb^\xba\xe4\x8f\x97g\xbeK\x8e\x8dd\x07W8Ui46=\x13H\xefO\xe2\xd3f\x1e\xa4\xcf\x1a\xb7^63\xd4\xb4\x06\xbd\x18\x01\xfdw\x84P\x00E\xf5\x8c]\xe8#\xbc~\xfe5\xe1\xedP{\xa90\x8d\x19\xd3\t\x8ehg]\xbf<\x97\x18S\xbb)b\xc5\xca^r\xc1\xc7\x96\xd4\xdb-\xa0\xb4\x1fi\x03\xec\xea\xe2P\xf1\x0c<\xf0\xac\xf3S-\xf0\x1c\xf5\xedl99s\x80\x16\xc8R\xb0#\xcd\xe0>\xdc\xdd<G\xa0\xbb5\x8a\xe2\x98h\x8b\xbe\xe5\xbfr\xee\xd2\xfa\xa5\xed\x12\xed\xfc\x98\x18\xa9&v\xdc(K\x10 \x1c\xd3\x7f\x16w-\xedo\x80\xf7I\xbbS\x05\xbb]h\xc7\xd4\xc8u\x16?\x89Z\x8b\xf7\x17G\xd4L\xf1\xd2\x89y>M=\x98\xa8a\xde:\x1e\xd2\xf8^\x03\xe0\xc1\xc9\x1c\x8c\xd3\x8dM\xd3\x956\xb37_cc\x9b3\x14\xf0-&kS|\x89\x8c2\xc2n\xec=!\x009\xc9\xa1h\xe2P\x83.\xb0:+\xf36\xa0\xac/\xe4oa\xc2Q\t9>\x8bS\xb9\xbbg\xda\xdcS\xb9vY6\x9dC\xe5 \xe0=2`\x85\"Q\xb7\xc73\xbb\xdd\x15/\xa4x\xa6\x07{\x81F6\x04\x86\xddy5\xc7\x95,;\xb0\xa3\x175\xe5s\x1f\xb4\\Y\xef\xda\xea\x10e{z\xd0\x7f\x9f\xb3\xb4*7;p\x8b\x9b[\xb9+\xb7\xec\xb2Q\x12\x97S)Z\xd4\xf0\x12\x10\xdcO\x02\xbb\x12\x92/b\xd4?iC|\r\xd6\xfcXu\x01\x88\x9dX\x16K\xde\xba\x90\xffG\x01\x89\x06j\xf6_\xb2\x90j\xb3\x02\xa6\x02\x88\xbf\xb3G~*\xd9\xd5\xfahx5M\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=CA Disig Root R2 O=Disig a.s.
     * Subject: CN=CA Disig Root R2 O=Disig a.s.
     * Label: "CA Disig Root R2"
     * Serial: 10572350602393338211
     * MD5 Fingerprint: 26:01:fb:d8:27:a7:17:9a:45:54:38:1a:43:01:3b:03
     * SHA1 Fingerprint: b5:61:eb:ea:a4:de:e4:25:4b:69:1a:98:a5:57:47:c2:34:c7:d9:71
     * SHA256 Fingerprint: e2:3d:4a:03:6d:7b:70:e9:f5:95:b1:42:20:79:d2:b9:1e:df:bb:1f:b6:51:a0:63:3e:aa:8a:9d:c5:f8:07:03
     * -----BEGIN CERTIFICATE-----
     * MIIFaTCCA1GgAwIBAgIJAJK4iNuwisFjMA0GCSqGSIb3DQEBCwUAMFIxCzAJBgNV
     * BAYTAlNLMRMwEQYDVQQHEwpCcmF0aXNsYXZhMRMwEQYDVQQKEwpEaXNpZyBhLnMu
     * MRkwFwYDVQQDExBDQSBEaXNpZyBSb290IFIyMB4XDTEyMDcxOTA5MTUzMFoXDTQy
     * MDcxOTA5MTUzMFowUjELMAkGA1UEBhMCU0sxEzARBgNVBAcTCkJyYXRpc2xhdmEx
     * EzARBgNVBAoTCkRpc2lnIGEucy4xGTAXBgNVBAMTEENBIERpc2lnIFJvb3QgUjIw
     * ggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIKAoICAQCio8QACdaFXS1tFPbCw3Oe
     * NcJxVX6B+6tGUODBfEl45qt5WDza/3wcn9iXAng+a0EE6UG9vgMsRfYvZNSrXaNH
     * PWSb6WiaxswbP7q+sos0Ai6YVRn8jG+qX9pMzk0DIaPY0jSTVpbLTAwAFjxfGs3I
     * x2ymrdMxp7zo5eFm1tL7A7RBZckQrg4FY8aAamkw/dLukO8NJ9+flXP04SXabBbe
     * QTg06ov80egEFGEtQX6sx3dOy1FU+16SGBsEWmjGycT6txOgmLcRK7fWV8x8nhfR
     * yyX+hk4kLlYMeE2eARKmK6cBZW58Yh2EhN/qwGu1pSqVg8NTEQxzHQuyRpDRQjrO
     * QG6Vrf/GlK1ul4SOfW+eioANSW1z4nuSHsPzwfPrLgVv2RvPN3YEyLRa5Beny912
     * H9AZdugsBbPWnDTYltxhh5EF5EQIM8HauQhl1K6yNg3ruji6DOWbnuuNZt2Zz9aJ
     * QfYEkoopKW1rOhzndX0CcQ7zwOe9yxndnWCywmZgtrEE7snmhrmaZkCo5xHtgUUD
     * i/ZnWejBBhG93c+AAk9lQHhcR1DIm+YfgXvkRKhbhZri3lrVx/k6RGZL5DJUfORs
     * nLMOPReisjQS1n6yqEm70XooQL6iFh/f5DcfEXP7kAplQ6INfPgGAVUzfbANuPT1
     * rqVCV3w2EYx7XsQDnYx5nQIDAQABo0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1Ud
     * DwEB/wQEAwIBBjAdBgNVHQ4EFgQUtZn4r7CU9eMg1gqtzk5WpC5uQu0wDQYJKoZI
     * hvcNAQELBQADggIBACYGXnDnZTPIgm7ZnBc6G3pmsgH2eDtpXi/q/075KMOYKmFM
     * tCQSin1tERT3nLXK5ryeJ45MGcipvXrA1zYObYVybqjGom32+nNjf7xueQgcnYqf
     * GopTpti72TVVsRHFqQOzVju5hJMiXn7B9hJSi+osZ7z+Nkz1uM/Rs0mSO9MpDpkb
     * lvdhuDvEK7Z4bLQjb/D907JedR+Zlais9trhxTF7+9FGs9K8Z7RiVLoJ92Owk6Ka
     * +elSLotgEqv89WBW7xBci8QaQtyDW2QOy7W81k/BfDxujRNt+3vrMNDcTa/F1bal
     * TFtxyegxvug4BkihGuLq0t4SOVga/4AOgnXmt8kHbA7v/zjxmHHEt38OFdAlab0i
     * nSvtBfZGR6ztwPDUO+Ls7pZbkBNOHlY667DvlruWIxG68kOGdGSVyCh13x01utI3
     * gzhTODY7z2zp+WsO0PsE6E9312UBeIYMej4hYvF/Y3EMyZ9E26gnonW+boE+18Dr
     * G5gPcFw0sorMwIUY6256s/daoQe/qUKS82Ail+QUoQebTnbAjn39pCXHR+3/H3Os
     * zMOl6W8KjptlwlCFtaOgUxLMVYdh84GuEEZhvUQhuMI9dM9+JDX6HAcOmz0iyu8x
     * L4ysEr3vQCj8KWefshNPZiTEUxnpHikV7+ZtsH8tZ/3zbBt1RqPlShfppNcL
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02SK1\x130\x11\x06\x03U\x04\x07\x13\nBratislava1\x130\x11\x06\x03U\x04\n\x13\nDisig a.s.1\x190\x17\x06\x03U\x04\x03\x13\x10CA Disig Root R2",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa2\xa3\xc4\x00\t\xd6\x85]-m\x14\xf6\xc2\xc3s\x9e5\xc2qU~\x81\xfb\xabFP\xe0\xc1|Ix\xe6\xabyX<\xda\xff|\x1c\x9f\xd8\x97\x02x>kA\x04\xe9A\xbd\xbe\x03,E\xf6/d\xd4\xab]\xa3G=d\x9b\xe9h\x9a\xc6\xcc\x1b?\xba\xbe\xb2\x8b4\x02.\x98U\x19\xfc\x8co\xaa_\xdaL\xceM\x03!\xa3\xd8\xd24\x93V\x96\xcbL\x0c\x00\x16<_\x1a\xcd\xc8\xc7l\xa6\xad\xd31\xa7\xbc\xe8\xe5\xe1f\xd6\xd2\xfb\x03\xb4Ae\xc9\x10\xae\x0e\x05c\xc6\x80ji0\xfd\xd2\xee\x90\xef\r\'\xdf\x9f\x95s\xf4\xe1%\xdal\x16\xdeA84\xea\x8b\xfc\xd1\xe8\x04\x14a-A~\xac\xc7wN\xcbQT\xfb^\x92\x18\x1b\x04Zh\xc6\xc9\xc4\xfa\xb7\x13\xa0\x98\xb7\x11+\xb7\xd6W\xcc|\x9e\x17\xd1\xcb%\xfe\x86N$.V\x0cxM\x9e\x01\x12\xa6+\xa7\x01en|b\x1d\x84\x84\xdf\xea\xc0k\xb5\xa5*\x95\x83\xc3S\x11\x0cs\x1d\x0b\xb2F\x90\xd1B:\xce@n\x95\xad\xff\xc6\x94\xadn\x97\x84\x8e}o\x9e\x8a\x80\rIms\xe2{\x92\x1e\xc3\xf3\xc1\xf3\xeb.\x05o\xd9\x1b\xcf7v\x04\xc8\xb4Z\xe4\x17\xa7\xcb\xddv\x1f\xd0\x19v\xe8,\x05\xb3\xd6\x9c4\xd8\x96\xdca\x87\x91\x05\xe4D\x083\xc1\xda\xb9\x08e\xd4\xae\xb26\r\xeb\xba8\xba\x0c\xe5\x9b\x9e\xeb\x8df\xdd\x99\xcf\xd6\x89A\xf6\x04\x92\x8a))mk:\x1c\xe7u}\x02q\x0e\xf3\xc0\xe7\xbd\xcb\x19\xdd\x9d`\xb2\xc2f`\xb6\xb1\x04\xee\xc9\xe6\x86\xb9\x9af@\xa8\xe7\x11\xed\x81E\x03\x8b\xf6gY\xe8\xc1\x06\x11\xbd\xdd\xcf\x80\x02Oe@x\\GP\xc8\x9b\xe6\x1f\x81{\xe4D\xa8[\x85\x9a\xe2\xdeZ\xd5\xc7\xf9:DfK\xe42T|\xe4l\x9c\xb3\x0e=\x17\xa2\xb24\x12\xd6~\xb2\xa8I\xbb\xd1z(@\xbe\xa2\x16\x1f\xdf\xe47\x1f\x11s\xfb\x90\neC\xa2\r|\xf8\x06\x01U3}\xb0\r\xb8\xf4\xf5\xae\xa5BW|6\x11\x8c{^\xc4\x03\x9d\x8cy\x9d\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Amazon Root CA 4 O=Amazon
     * Subject: CN=Amazon Root CA 4 O=Amazon
     * Label: "Amazon Root CA 4"
     * Serial: 143266989758080763974105200630763877849284878
     * MD5 Fingerprint: 89:bc:27:d5:eb:17:8d:06:6a:69:d5:fd:89:47:b4:cd
     * SHA1 Fingerprint: f6:10:84:07:d6:f8:bb:67:98:0c:c2:e2:44:c2:eb:ae:1c:ef:63:be
     * SHA256 Fingerprint: e3:5d:28:41:9e:d0:20:25:cf:a6:90:38:cd:62:39:62:45:8d:a5:c6:95:fb:de:a3:c2:2b:0b:fb:25:89:70:92
     * -----BEGIN CERTIFICATE-----
     * MIIB8jCCAXigAwIBAgITBmyf18G7EEwpQ+Vxe3ssyBrBDjAKBggqhkjOPQQDAzA5
     * MQswCQYDVQQGEwJVUzEPMA0GA1UEChMGQW1hem9uMRkwFwYDVQQDExBBbWF6b24g
     * Um9vdCBDQSA0MB4XDTE1MDUyNjAwMDAwMFoXDTQwMDUyNjAwMDAwMFowOTELMAkG
     * A1UEBhMCVVMxDzANBgNVBAoTBkFtYXpvbjEZMBcGA1UEAxMQQW1hem9uIFJvb3Qg
     * Q0EgNDB2MBAGByqGSM49AgEGBSuBBAAiA2IABNKrijdPo1MN/sGKe0uoe0ZLY7Bi
     * 9i0b2whxIdIA6GO9mif78DluXeo9pcmBqqNbIJhFXRbb/egQbeOc4OO9X4Ri83Bk
     * M6DLJC9wuoihKqB1+IGuYgbEgds5bimwHvouXKNCMEAwDwYDVR0TAQH/BAUwAwEB
     * /zAOBgNVHQ8BAf8EBAMCAYYwHQYDVR0OBBYEFNPsxzplbszh2naaVvuc84ZtV+WB
     * MAoGCCqGSM49BAMDA2gAMGUCMDqLIfG9fhGt0O9Yli/W651+kI0rz2ZVwyzjKKlw
     * CkcO8DdZEv8tmZQoTipPNU0zWgIxAOp1AE47xDqUEpHJWEadIRNyp4iciuRMStuW
     * 1KyLa2tJElMzrdfkviT8tQp21KW8EA==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x0f0\r\x06\x03U\x04\n\x13\x06Amazon1\x190\x17\x06\x03U\x04\x03\x13\x10Amazon Root CA 4",
        spki: b"0\x10\x06\x07*\x86H\xce=\x02\x01\x06\x05+\x81\x04\x00\"\x03b\x00\x04\xd2\xab\x8a7O\xa3S\r\xfe\xc1\x8a{K\xa8{FKc\xb0b\xf6-\x1b\xdb\x08q!\xd2\x00\xe8c\xbd\x9a\'\xfb\xf09n]\xea=\xa5\xc9\x81\xaa\xa3[ \x98E]\x16\xdb\xfd\xe8\x10m\xe3\x9c\xe0\xe3\xbd_\x84b\xf3pd3\xa0\xcb$/p\xba\x88\xa1*\xa0u\xf8\x81\xaeb\x06\xc4\x81\xdb9n)\xb0\x1e\xfa.\\",
        name_constraints: None
    },

    /*
     * Issuer: CN=Certigna O=Dhimyotis
     * Subject: CN=Certigna O=Dhimyotis
     * Label: "Certigna"
     * Serial: 18364802974209362175
     * MD5 Fingerprint: ab:57:a6:5b:7d:42:82:19:b5:d8:58:26:28:5e:fd:ff
     * SHA1 Fingerprint: b1:2e:13:63:45:86:a4:6f:1a:b2:60:68:37:58:2d:c4:ac:fd:94:97
     * SHA256 Fingerprint: e3:b6:a2:db:2e:d7:ce:48:84:2f:7a:c5:32:41:c7:b7:1d:54:14:4b:fb:40:c1:1f:3f:1d:0b:42:f5:ee:a1:2d
     * -----BEGIN CERTIFICATE-----
     * MIIDqDCCApCgAwIBAgIJAP7c4wEPyUj/MA0GCSqGSIb3DQEBBQUAMDQxCzAJBgNV
     * BAYTAkZSMRIwEAYDVQQKDAlEaGlteW90aXMxETAPBgNVBAMMCENlcnRpZ25hMB4X
     * DTA3MDYyOTE1MTMwNVoXDTI3MDYyOTE1MTMwNVowNDELMAkGA1UEBhMCRlIxEjAQ
     * BgNVBAoMCURoaW15b3RpczERMA8GA1UEAwwIQ2VydGlnbmEwggEiMA0GCSqGSIb3
     * DQEBAQUAA4IBDwAwggEKAoIBAQDIaPHJ1tazNHUmgh7stL7qXOEm7RFHYeGifBZ4
     * QCHkYJ5ayGPhxLGWkv8YbWkj4Sti993iNi+RB7lIzw7sebYs5zRLcAglozyHGxny
     * gQcPOJAZ0xH+hrTy0V4eHpbNgGzOOzGTtvKg0KmVEn2lmsxryIRWijOp5yIVUxbw
     * zBfsV1/pogqYCd7jX5xv3EjjhQsVWqa6n6xI4wmy9/Qy3l40vhx4XUJbzg4ij02Q
     * 130yGLMLLGq/jj8UEYkgDncUtT2UCIf3JR7VsmAA7G8qKCVuKj4YYxclPz5EIBb2
     * JsglrgVKtOdjLPOMFlN+XPsRGgjBRmKfIrjxwo1p3Po6WAbfAgMBAAGjgbwwgbkw
     * DwYDVR0TAQH/BAUwAwEB/zAdBgNVHQ4EFgQUGu3+QTmQtCRZvgHyUtVF9lo53BEw
     * ZAYDVR0jBF0wW4AUGu3+QTmQtCRZvgHyUtVF9lo53BGhOKQ2MDQxCzAJBgNVBAYT
     * AkZSMRIwEAYDVQQKDAlEaGlteW90aXMxETAPBgNVBAMMCENlcnRpZ25hggkA/tzj
     * AQ/JSP8wDgYDVR0PAQH/BAQDAgEGMBEGCWCGSAGG+EIBAQQEAwIABzANBgkqhkiG
     * 9w0BAQUFAAOCAQEAhQMeknH2Qq/ho2Ge6/PAD/Kl1NqV5ta+aDY9fm4fTIrv0Q8h
     * bV6lUmPOEvjvKtpv6zf+EwLHyzs+ImvaYS5/1HI93TDhHkxAGYwP15zRgzB7mFnc
     * fca5DClMoTOi62c6ZYTTluLtdkVwj7Ur3vkj1kluPBS1xp81HlDQwY9qcEQCYsuu
     * HWhBp6pX6FOqB9IG9tUUBguRA3UsbHK1YZWaDYu5Def131TN3ubY1gkIl2PlwS6w
     * t0QmwCbAr1UwnjvVNioZBPRcHv/PLLf/0P2HQBHVESO7SMAhqaQoLf0V+LBOK/Qw
     * WyH8EZE0vkHve52Xdf+XlcCWWC/qu0bXu+TZLg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02FR1\x120\x10\x06\x03U\x04\n\x0c\tDhimyotis1\x110\x0f\x06\x03U\x04\x03\x0c\x08Certigna",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xc8h\xf1\xc9\xd6\xd6\xb34u&\x82\x1e\xec\xb4\xbe\xea\\\xe1&\xed\x11Ga\xe1\xa2|\x16x@!\xe4`\x9eZ\xc8c\xe1\xc4\xb1\x96\x92\xff\x18mi#\xe1+b\xf7\xdd\xe26/\x91\x07\xb9H\xcf\x0e\xecy\xb6,\xe74Kp\x08%\xa3<\x87\x1b\x19\xf2\x81\x07\x0f8\x90\x19\xd3\x11\xfe\x86\xb4\xf2\xd1^\x1e\x1e\x96\xcd\x80l\xce;1\x93\xb6\xf2\xa0\xd0\xa9\x95\x12}\xa5\x9a\xcck\xc8\x84V\x8a3\xa9\xe7\"\x15S\x16\xf0\xcc\x17\xecW_\xe9\xa2\n\x98\t\xde\xe3_\x9co\xdcH\xe3\x85\x0b\x15Z\xa6\xba\x9f\xacH\xe3\t\xb2\xf7\xf42\xde^4\xbe\x1cx]B[\xce\x0e\"\x8fM\x90\xd7}2\x18\xb3\x0b,j\xbf\x8e?\x14\x11\x89 \x0ew\x14\xb5=\x94\x08\x87\xf7%\x1e\xd5\xb2`\x00\xeco*(%n*>\x18c\x17%?>D \x16\xf6&\xc8%\xae\x05J\xb4\xe7c,\xf3\x8c\x16S~\\\xfb\x11\x1a\x08\xc1Fb\x9f\"\xb8\xf1\xc2\x8di\xdc\xfa:X\x06\xdf\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=SECOM Trust.net OU=Security Communication RootCA1
     * Subject: O=SECOM Trust.net OU=Security Communication RootCA1
     * Label: "Security Communication Root CA"
     * Serial: 0
     * MD5 Fingerprint: f1:bc:63:6a:54:e0:b5:27:f5:cd:e7:1a:e3:4d:6e:4a
     * SHA1 Fingerprint: 36:b1:2b:49:f9:81:9e:d7:4c:9e:bc:38:0f:c6:56:8f:5d:ac:b2:f7
     * SHA256 Fingerprint: e7:5e:72:ed:9f:56:0e:ec:6e:b4:80:00:73:a4:3f:c3:ad:19:19:5a:39:22:82:01:78:95:97:4a:99:02:6b:6c
     * -----BEGIN CERTIFICATE-----
     * MIIDWjCCAkKgAwIBAgIBADANBgkqhkiG9w0BAQUFADBQMQswCQYDVQQGEwJKUDEY
     * MBYGA1UEChMPU0VDT00gVHJ1c3QubmV0MScwJQYDVQQLEx5TZWN1cml0eSBDb21t
     * dW5pY2F0aW9uIFJvb3RDQTEwHhcNMDMwOTMwMDQyMDQ5WhcNMjMwOTMwMDQyMDQ5
     * WjBQMQswCQYDVQQGEwJKUDEYMBYGA1UEChMPU0VDT00gVHJ1c3QubmV0MScwJQYD
     * VQQLEx5TZWN1cml0eSBDb21tdW5pY2F0aW9uIFJvb3RDQTEwggEiMA0GCSqGSIb3
     * DQEBAQUAA4IBDwAwggEKAoIBAQCzs/5/022x7xZ8V6UMbXaKL0u/ZPtM7orw8yl8
     * 9f/uKuDp6bpbZCKamm8sOiZpUQWZJtzVHGpxxpp9Hp3dfGzGjGdnSj74cbAZJ6kJ
     * DKaVv0uMDPpVmDvY6CKhS3E4eayXkmmziX7qIWgGmBSWh9JhNrxtJ1aeV+7AwFb9
     * Ms+k2Y7CI9eNqPPYJayX5HA49LY6tJ07lyZDo6G8SVlyTCMwhwFY9k6+HGhWZq/N
     * QV3Is00qVUarH9oe4kA92819uZKAnDfdDJZkndwi92SL32HeFZRSFaB9UslLqCHJ
     * xrHty8OVYNEP8Ktw+N/LTX7s1vqr2b1/VPKl6Xn62dZ2JChzAgMBAAGjPzA9MB0G
     * A1UdDgQWBBSgc0mZaNyFW2XjmygvV5+9M7wHSDALBgNVHQ8EBAMCAQYwDwYDVR0T
     * AQH/BAUwAwEB/zANBgkqhkiG9w0BAQUFAAOCAQEAaECpqLvkT115swW1F7NgE+vG
     * kl3g0dNq/vu+m22/xwVtWSDEHPC32oRYAmP6SBbvT6UL90qY8j+eG61Ha2POCEfr
     * Uj94nK9NrvjVT8+amCoQQTlSxN3Zmw7vkwGusi7KaEIkQmywszo+zenaSMQVy+n5
     * Bw+SUEmK3TGXX8npN6o7WWWXlDLJs58+OmJYxUmtYg5xpTKqL8aJdkNAExNnPaJU
     * JRDL8Try2frbSVa7pv6nQTXD4IhhyYjH3zYQIphZ6rBK+1YWc26sTfcioU+tHXot
     * RSflMMFe8toTyyVCUZVHA4xsIcx0Qu1T/zOLjw9XARYvz6buyXAiFL39vmwLAw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02JP1\x180\x16\x06\x03U\x04\n\x13\x0fSECOM Trust.net1\'0%\x06\x03U\x04\x0b\x13\x1eSecurity Communication RootCA1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb3\xb3\xfe\x7f\xd3m\xb1\xef\x16|W\xa5\x0cmv\x8a/K\xbfd\xfbL\xee\x8a\xf0\xf3)|\xf5\xff\xee*\xe0\xe9\xe9\xba[d\"\x9a\x9ao,:&iQ\x05\x99&\xdc\xd5\x1cjq\xc6\x9a}\x1e\x9d\xdd|l\xc6\x8cggJ>\xf8q\xb0\x19\'\xa9\t\x0c\xa6\x95\xbfK\x8c\x0c\xfaU\x98;\xd8\xe8\"\xa1Kq8y\xac\x97\x92i\xb3\x89~\xea!h\x06\x98\x14\x96\x87\xd2a6\xbcm\'V\x9eW\xee\xc0\xc0V\xfd2\xcf\xa4\xd9\x8e\xc2#\xd7\x8d\xa8\xf3\xd8%\xac\x97\xe4p8\xf4\xb6:\xb4\x9d;\x97&C\xa3\xa1\xbcIYrL#0\x87\x01X\xf6N\xbe\x1chVf\xaf\xcdA]\xc8\xb3M*UF\xab\x1f\xda\x1e\xe2@=\xdb\xcd}\xb9\x92\x80\x9c7\xdd\x0c\x96d\x9d\xdc\"\xf7d\x8b\xdfa\xde\x15\x94R\x15\xa0}R\xc9K\xa8!\xc9\xc6\xb1\xed\xcb\xc3\x95`\xd1\x0f\xf0\xabp\xf8\xdf\xcbM~\xec\xd6\xfa\xab\xd9\xbd\x7fT\xf2\xa5\xe9y\xfa\xd9\xd6v$(s\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=USERTrust RSA Certification Authority O=The USERTRUST Network
     * Subject: CN=USERTrust RSA Certification Authority O=The USERTRUST Network
     * Label: "USERTrust RSA Certification Authority"
     * Serial: 2645093764781058787591871645665788717
     * MD5 Fingerprint: 1b:fe:69:d1:91:b7:19:33:a3:72:a8:0f:e1:55:e5:b5
     * SHA1 Fingerprint: 2b:8f:1b:57:33:0d:bb:a2:d0:7a:6c:51:f7:0e:e9:0d:da:b9:ad:8e
     * SHA256 Fingerprint: e7:93:c9:b0:2f:d8:aa:13:e2:1c:31:22:8a:cc:b0:81:19:64:3b:74:9c:89:89:64:b1:74:6d:46:c3:d4:cb:d2
     * -----BEGIN CERTIFICATE-----
     * MIIF3jCCA8agAwIBAgIQAf1tMPyjylGoG7xkDjUDLTANBgkqhkiG9w0BAQwFADCB
     * iDELMAkGA1UEBhMCVVMxEzARBgNVBAgTCk5ldyBKZXJzZXkxFDASBgNVBAcTC0pl
     * cnNleSBDaXR5MR4wHAYDVQQKExVUaGUgVVNFUlRSVVNUIE5ldHdvcmsxLjAsBgNV
     * BAMTJVVTRVJUcnVzdCBSU0EgQ2VydGlmaWNhdGlvbiBBdXRob3JpdHkwHhcNMTAw
     * MjAxMDAwMDAwWhcNMzgwMTE4MjM1OTU5WjCBiDELMAkGA1UEBhMCVVMxEzARBgNV
     * BAgTCk5ldyBKZXJzZXkxFDASBgNVBAcTC0plcnNleSBDaXR5MR4wHAYDVQQKExVU
     * aGUgVVNFUlRSVVNUIE5ldHdvcmsxLjAsBgNVBAMTJVVTRVJUcnVzdCBSU0EgQ2Vy
     * dGlmaWNhdGlvbiBBdXRob3JpdHkwggIiMA0GCSqGSIb3DQEBAQUAA4ICDwAwggIK
     * AoICAQCAEmUXNg7D2wiz0KxXDXbtzSfTTK1Qg2HiqiBNCS1kCdzOiZ/MPans9s/B
     * 3PHTsdZ7NygRK0faOca8Ohm0X6a9fZ2jY0K2dvKpOyuR+OJv0OwWIJAJPuLodMkY
     * tJHUYmTbf6MG8YgYapAiPLz+E/CHFHv25B+O1ORRxhFnRghRy4YUVD+8M/5+bJz/
     * Fp0YvVGONaanZshyZ9shZrHUm3gDwFA66Mzw3LyeTP6vBZY1H1dat//O+T23LLb2
     * VN3I5xI6Ta5MirdcmrS3ID3KfyI0rn47aGYBROcBTkZTmzNg95S+UzeQc0PzMsNT
     * 79uq/nROacdrjGCT3sTHDN/hMq7MkztReJVni+49Vv4M0GkPGw/zJSZrM233bkf6
     * c0Plfg6lZrEpfDKEY1WJxA3Bk1QwGROs0303p+tdOmw1XNtB1xLaqUkL39iAigmT
     * Yo61Zs8liM2EuLE/pDkP2QKe6xJMlXzzawWpXhaDzLhn4ugTncxbgtNMs+1b/97l
     * c6wjOy0AvzVVdAlJ2ElYGn+SNuZRkg7zJn0cTRe8yexDJtC/QV9AqURE9JnnV4ee
     * UB9XVKg+/XRjL7FQZQnmWEIuQxpMtPAlR1n6BB6T1CZGSlCBst6+eLf8ZxXhyVeE
     * Hg9j1uliutZfVS7qXMYoCAQlObgOK6nyTJccBz8NUvXt7y+CDwIDAQABo0IwQDAd
     * BgNVHQ4EFgQUU3m/WqorSs9UgOHYm8Cd8rIDZsswDgYDVR0PAQH/BAQDAgEGMA8G
     * A1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQEMBQADggIBAFzUfA3P9wF9QZllDHPF
     * Up/L+M+ZBn8b2kMVn54CVVeWFPFSPCeHlCjtHzoBN6J2/FNQwISbxmtOuowhT6KO
     * VWKR82kV2LyI48SqC/3vqOlLVSoGIG1VeCkZ7l8wXEskEVX/JJpuXior7gtNn3/3
     * ATiUFJVDBwn7YKnuHKsSjKCaXqeYalltiz8I+8jRRa8YFWSQEg9zKC7F4iRO/Fjs
     * 8PRF/iKz6y+O0tlFYQXBl2+odnKPi4w2r78NBc5xjeambx9spnFixdjQg3IM8WcR
     * iQycE0xyNN+81XHfqnHd4blsjDwSXWXavVcStkNr/+XeTWYRUc+ZruwXtuhxkYze
     * Sf7dNXGiFSeUHM9h4ya7b6NnJSFd5t0dCy5oGzuCr+yDZ4XUmFF0sbmZgIn/f3gZ
     * XHlKYC6SQK5MNyosycdiyA5d9zZbyuAlJQG03RoHnHcAP9Dc1ew91Pq7P8yF1m9/
     * qS3fuQL39ZeatTXaw2ewh0qpKJ4jjv9cJ2vhsE/zB+4ALtRZh8tSQZXq9EfX7mRB
     * VXyNWQKV3WKdwrnuWih0hKWbt5DHDAff9Yk2dDLWKMGwsAvgnEzDHNb842m1R0aB
     * L6KCq9NjRHDEjf8tM7qtj3u1cIiuPhnPQCjY/MiQu12ZIvVS5ljFH4gxQ+6IHdfG
     * jjxDah2nGN59PRbxYvnKkKj9
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1\x130\x11\x06\x03U\x04\x08\x13\nNew Jersey1\x140\x12\x06\x03U\x04\x07\x13\x0bJersey City1\x1e0\x1c\x06\x03U\x04\n\x13\x15The USERTRUST Network1.0,\x06\x03U\x04\x03\x13%USERTrust RSA Certification Authority",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\x80\x12e\x176\x0e\xc3\xdb\x08\xb3\xd0\xacW\rv\xed\xcd\'\xd3L\xadP\x83a\xe2\xaa M\t-d\t\xdc\xce\x89\x9f\xcc=\xa9\xec\xf6\xcf\xc1\xdc\xf1\xd3\xb1\xd6{7(\x11+G\xda9\xc6\xbc:\x19\xb4_\xa6\xbd}\x9d\xa3cB\xb6v\xf2\xa9;+\x91\xf8\xe2o\xd0\xec\x16 \x90\t>\xe2\xe8t\xc9\x18\xb4\x91\xd4bd\xdb\x7f\xa3\x06\xf1\x88\x18j\x90\"<\xbc\xfe\x13\xf0\x87\x14{\xf6\xe4\x1f\x8e\xd4\xe4Q\xc6\x11gF\x08Q\xcb\x86\x14T?\xbc3\xfe~l\x9c\xff\x16\x9d\x18\xbdQ\x8e5\xa6\xa7f\xc8rg\xdb!f\xb1\xd4\x9bx\x03\xc0P:\xe8\xcc\xf0\xdc\xbc\x9eL\xfe\xaf\x05\x965\x1fWZ\xb7\xff\xce\xf9=\xb7,\xb6\xf6T\xdd\xc8\xe7\x12:M\xaeL\x8a\xb7\\\x9a\xb4\xb7 =\xca\x7f\"4\xae~;hf\x01D\xe7\x01NFS\x9b3`\xf7\x94\xbeS7\x90sC\xf32\xc3S\xef\xdb\xaa\xfetNi\xc7k\x8c`\x93\xde\xc4\xc7\x0c\xdf\xe12\xae\xcc\x93;Qx\x95g\x8b\xee=V\xfe\x0c\xd0i\x0f\x1b\x0f\xf3%&k3m\xf7nG\xfasC\xe5~\x0e\xa5f\xb1)|2\x84cU\x89\xc4\r\xc1\x93T0\x19\x13\xac\xd3}7\xa7\xeb]:l5\\\xdbA\xd7\x12\xda\xa9I\x0b\xdf\xd8\x80\x8a\t\x93b\x8e\xb5f\xcf%\x88\xcd\x84\xb8\xb1?\xa49\x0f\xd9\x02\x9e\xeb\x12L\x95|\xf3k\x05\xa9^\x16\x83\xcc\xb8g\xe2\xe8\x13\x9d\xcc[\x82\xd3L\xb3\xed[\xff\xde\xe5s\xac#;-\x00\xbf5Ut\tI\xd8IX\x1a\x7f\x926\xe6Q\x92\x0e\xf3&}\x1cM\x17\xbc\xc9\xecC&\xd0\xbfA_@\xa9DD\xf4\x99\xe7W\x87\x9eP\x1fWT\xa8>\xfdtc/\xb1Pe\t\xe6XB.C\x1aL\xb4\xf0%GY\xfa\x04\x1e\x93\xd4&FJP\x81\xb2\xde\xbex\xb7\xfcg\x15\xe1\xc9W\x84\x1e\x0fc\xd6\xe9b\xba\xd6_U.\xea\\\xc6(\x08\x04%9\xb8\x0e+\xa9\xf2L\x97\x1c\x07?\rR\xf5\xed\xef/\x82\x0f\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=certSIGN OU=certSIGN ROOT CA
     * Subject: O=certSIGN OU=certSIGN ROOT CA
     * Label: "certSIGN ROOT CA"
     * Serial: 35210227249154
     * MD5 Fingerprint: 18:98:c0:d6:e9:3a:fc:f9:b0:f5:0c:f7:4b:01:44:17
     * SHA1 Fingerprint: fa:b7:ee:36:97:26:62:fb:2d:b0:2a:f6:bf:03:fd:e8:7c:4b:2f:9b
     * SHA256 Fingerprint: ea:a9:62:c4:fa:4a:6b:af:eb:e4:15:19:6d:35:1c:cd:88:8d:4f:53:f3:fa:8a:e6:d7:c4:66:a9:4e:60:42:bb
     * -----BEGIN CERTIFICATE-----
     * MIIDODCCAiCgAwIBAgIGIAYFFnACMA0GCSqGSIb3DQEBBQUAMDsxCzAJBgNVBAYT
     * AlJPMREwDwYDVQQKEwhjZXJ0U0lHTjEZMBcGA1UECxMQY2VydFNJR04gUk9PVCBD
     * QTAeFw0wNjA3MDQxNzIwMDRaFw0zMTA3MDQxNzIwMDRaMDsxCzAJBgNVBAYTAlJP
     * MREwDwYDVQQKEwhjZXJ0U0lHTjEZMBcGA1UECxMQY2VydFNJR04gUk9PVCBDQTCC
     * ASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBALczuX7IJUqOtdu0KBuqV5Do
     * 0SLTZLrTk+jUrIZhQGpgV2hUhE28alQCBf/fm5oqrl0Hj0rDKH/v+yv6efHHrfAQ
     * UySQi2bJqIirr1qjAOm+ukbuW3N7LBeCgV5iLKECZbO9xSsAfsT8AzNXDe3i+s5d
     * RdY4zTW2ssHQnIFKquSyAVwdj1+ZxLGt24gh65AIgoDzMKND5pCCrlUoSe1b16kQ
     * OA7+j0xbm0bqQfWwCHTD0IgztnzXdN/chNFDDnU5oSVAKOp4yw4sLjmdjItuFhwv
     * JoIQ4uNllAoEwF73XVv4EOLQunpL+943AAAaWyjj0pxzPjKHmKHJUS/X3qwzs08C
     * AwEAAaNCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAcYwHQYDVR0O
     * BBYEFOCMm9slSbPxfIbWskKHC9BroNnkMA0GCSqGSIb3DQEBBQUAA4IBAQA+0hyJ
     * LjX8+HXd5n9liPRyTMks1zJO890ZeUe9jjtbkw9QSSQTaxQGcu8J06Gh40CEyecY
     * MnQ8SG4Pn0vU9x7Tk4ZkVJdjclDVVc/6IJMCopvDI5NOFlV2oHB5bc0hH88vLbwZ
     * 44gx+FkagQnIl6Z0x2DEW8xXjrJ1/RsCCdtZb3KTafcxQdaIOL+Hsr0Wefmq5L6I
     * Jd1hJyMctTEHBDa0GpC9oHRxUIltvBTjD4au8as+x6AJzKNI0eDbZOeStc+vckNw
     * i/nDhDwTqn6Sm1dTk/pwwpEOMfmbZ13pljheX7NzTogVZ96edhBiIL5VaZVDADlN
     * 9u6wWk5JRFRYX0KD
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02RO1\x110\x0f\x06\x03U\x04\n\x13\x08certSIGN1\x190\x17\x06\x03U\x04\x0b\x13\x10certSIGN ROOT CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xb73\xb9~\xc8%J\x8e\xb5\xdb\xb4(\x1b\xaaW\x90\xe8\xd1\"\xd3d\xba\xd3\x93\xe8\xd4\xac\x86a@j`WhT\x84M\xbcjT\x02\x05\xff\xdf\x9b\x9a*\xae]\x07\x8fJ\xc3(\x7f\xef\xfb+\xfay\xf1\xc7\xad\xf0\x10S$\x90\x8bf\xc9\xa8\x88\xab\xafZ\xa3\x00\xe9\xbe\xbaF\xee[s{,\x17\x82\x81^b,\xa1\x02e\xb3\xbd\xc5+\x00~\xc4\xfc\x033W\r\xed\xe2\xfa\xce]E\xd68\xcd5\xb6\xb2\xc1\xd0\x9c\x81J\xaa\xe4\xb2\x01\\\x1d\x8f_\x99\xc4\xb1\xad\xdb\x88!\xeb\x90\x08\x82\x80\xf30\xa3C\xe6\x90\x82\xaeU(I\xed[\xd7\xa9\x108\x0e\xfe\x8fL[\x9bF\xeaA\xf5\xb0\x08t\xc3\xd0\x883\xb6|\xd7t\xdf\xdc\x84\xd1C\x0eu9\xa1%@(\xeax\xcb\x0e,.9\x9d\x8c\x8bn\x16\x1c/&\x82\x10\xe2\xe3e\x94\n\x04\xc0^\xf7][\xf8\x10\xe2\xd0\xbazK\xfb\xde7\x00\x00\x1a[(\xe3\xd2\x9cs>2\x87\x98\xa1\xc9Q/\xd7\xde\xac3\xb3O\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: O=FNMT-RCM OU=AC RAIZ FNMT-RCM
     * Subject: O=FNMT-RCM OU=AC RAIZ FNMT-RCM
     * Label: "AC RAIZ FNMT-RCM"
     * Serial: 485876308206448804701554682760554759
     * MD5 Fingerprint: e2:09:04:b4:d3:bd:d1:a0:14:fd:1a:d2:47:c4:57:1d
     * SHA1 Fingerprint: ec:50:35:07:b2:15:c4:95:62:19:e2:a8:9a:5b:42:99:2c:4c:2c:20
     * SHA256 Fingerprint: eb:c5:57:0c:29:01:8c:4d:67:b1:aa:12:7b:af:12:f7:03:b4:61:1e:bc:17:b7:da:b5:57:38:94:17:9b:93:fa
     * -----BEGIN CERTIFICATE-----
     * MIIFgzCCA2ugAwIBAgIPXZONMGc2yAYdGsdUhGkHMA0GCSqGSIb3DQEBCwUAMDsx
     * CzAJBgNVBAYTAkVTMREwDwYDVQQKDAhGTk1ULVJDTTEZMBcGA1UECwwQQUMgUkFJ
     * WiBGTk1ULVJDTTAeFw0wODEwMjkxNTU5NTZaFw0zMDAxMDEwMDAwMDBaMDsxCzAJ
     * BgNVBAYTAkVTMREwDwYDVQQKDAhGTk1ULVJDTTEZMBcGA1UECwwQQUMgUkFJWiBG
     * Tk1ULVJDTTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBALpxgHpMhm5/
     * yBNtwMZ9HACXjywMI7sQmkCpGreHiPibVmr75nuOi5KOpyVdWRHbNi63URcfqQgf
     * BBckWKo3Shjf5TnUV/3XwSyRAZHiItQDwFj8d0fsjz50Q7qsNI1NOHZnjrDIbzAz
     * WHFctPVrbtQBULgTfmxKo0nRIBnuvMApGGWn3v7v3QqQIecaZ5JCEJhfTzC8PhxF
     * tBDXaEAUwED653cXeuYLj2VbPNmaUtu1vZ5Gzz3rkQUCwJaydkxNEJY7kvqcfw+Z
     * 374jNUUeAlz+taibmSXaXvMiwzn15Cou08YfxGyqxRxqAQVKL9LFwag0Jl1mpdIC
     * IfkYtwb1TplvqKtMUejPUBjFd8g5CSxJkjKZqLsXF3mwWsXmo8RZZUc1g16p6DUL
     * mbvkzSDGm0oGObVo/CK67lWMK07q87Hj/LaZmtVC+nFNCM+HHmpxffnTtOmlcYF7
     * wk5HlqX2doWjKI/pgG6BU6VtX7hI+cL5NqYuSf+4lsKMB7ObiFj86xsc3i1w4peS
     * MKGJ47xVqCfWS+2QrYv6YyVZLag13cqXM7zlzced0ezvXg5KkAYmY6252TUtB7p2
     * ZSysV4999AeU14ECll2jB0nVetBX+RvnU0Z1qrB5QstocQjpYL05ac70r8NWQMet
     * UqIJ5G+GR4of6ygnXYMgrwTJbFaai0b1AgMBAAGjgYMwgYAwDwYDVR0TAQH/BAUw
     * AwEB/zAOBgNVHQ8BAf8EBAMCAQYwHQYDVR0OBBYEFPd9xf3E6Jobd2Sn9R2gzL+H
     * YJptMD4GA1UdIAQ3MDUwMwYEVR0gADArMCkGCCsGAQUFBwIBFh1odHRwOi8vd3d3
     * LmNlcnQuZm5tdC5lcy9kcGNzLzANBgkqhkiG9w0BAQsFAAOCAgEAB5BK3/MjTvDD
     * nFFlm5wioooMhfNzKWtN/gHiqQxjAb8EZ6WdmF/9ARP67Jpi6Yb+tmLSbkyU+8B1
     * RXxlDPiyN8+sD8+Nb/kZ94/sHvJwnvDKuO+3/3Y3dlv2bojzr2IyIpMNOmqOFGYM
     * LVN0V2Ue1bLdI4E7pWYjJ2cJj+F3qkPNZVEI7VFY/uY5+ctHhKQV8Xa7pO6kO8Rf
     * 77IzlhEYt8llvhjho6Tc+hj507wTmzl6NLrTQfv6MooqtyuGC2mDOL7Nii4LcK2N
     * JpLuHvUBKwrZ1pebbuCoGRw6IYsMHkCtA+fdZn71uSANA+iW+YJF1DngoABd15jm
     * fZ5nc8OaKveri6E6FO80vFIOiZiaBECEHX5FaZNXzuvO+FB8TxxuBEOb+dY7Ixjp
     * 6o7RTUaN8Tvkasq6+yO3m/qZASlaWFot4/nUbQ4mrcFuNLwy+AwF+mWj2zs3gyLp
     * 1txyM/1d8iC9djwj2ij3+RvrWWTV3F9yfiD8zYm1kGdNYno/Tq0dwzn+evQoFt9B
     * 9kiABdcPUXmsEKvU7ANm5mqwujGSQkBqvjrTcuFqN1W8rB2Vt2lh8kORdOag0wok
     * RqEIr9baRRmW1FMdW4R58MD3R++Lj8UGrp1MYp3/RgT408m2ECVAdf4WqslKYIYv
     * uu8wd+RU4riEmViAqhOLUTpPSPaLtrM=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02ES1\x110\x0f\x06\x03U\x04\n\x0c\x08FNMT-RCM1\x190\x17\x06\x03U\x04\x0b\x0c\x10AC RAIZ FNMT-RCM",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xbaq\x80zL\x86n\x7f\xc8\x13m\xc0\xc6}\x1c\x00\x97\x8f,\x0c#\xbb\x10\x9a@\xa9\x1a\xb7\x87\x88\xf8\x9bVj\xfb\xe6{\x8e\x8b\x92\x8e\xa7%]Y\x11\xdb6.\xb7Q\x17\x1f\xa9\x08\x1f\x04\x17$X\xaa7J\x18\xdf\xe59\xd4W\xfd\xd7\xc1,\x91\x01\x91\xe2\"\xd4\x03\xc0X\xfcwG\xec\x8f>tC\xba\xac4\x8dM8vg\x8e\xb0\xc8o03Xq\\\xb4\xf5kn\xd4\x01P\xb8\x13~lJ\xa3I\xd1 \x19\xee\xbc\xc0)\x18e\xa7\xde\xfe\xef\xdd\n\x90!\xe7\x1ag\x92B\x10\x98_O0\xbc>\x1cE\xb4\x10\xd7h@\x14\xc0@\xfa\xe7w\x17z\xe6\x0b\x8fe[<\xd9\x9aR\xdb\xb5\xbd\x9eF\xcf=\xeb\x91\x05\x02\xc0\x96\xb2vLM\x10\x96;\x92\xfa\x9c\x7f\x0f\x99\xdf\xbe#5E\x1e\x02\\\xfe\xb5\xa8\x9b\x99%\xda^\xf3\"\xc39\xf5\xe4*.\xd3\xc6\x1f\xc4l\xaa\xc5\x1cj\x01\x05J/\xd2\xc5\xc1\xa84&]f\xa5\xd2\x02!\xf9\x18\xb7\x06\xf5N\x99o\xa8\xabLQ\xe8\xcfP\x18\xc5w\xc89\t,I\x922\x99\xa8\xbb\x17\x17y\xb0Z\xc5\xe6\xa3\xc4YeG5\x83^\xa9\xe85\x0b\x99\xbb\xe4\xcd \xc6\x9bJ\x069\xb5h\xfc\"\xba\xeeU\x8c+N\xea\xf3\xb1\xe3\xfc\xb6\x99\x9a\xd5B\xfaqM\x08\xcf\x87\x1ejq}\xf9\xd3\xb4\xe9\xa5q\x81{\xc2NG\x96\xa5\xf6v\x85\xa3(\x8f\xe9\x80n\x81S\xa5m_\xb8H\xf9\xc2\xf96\xa6.I\xff\xb8\x96\xc2\x8c\x07\xb3\x9b\x88X\xfc\xeb\x1b\x1c\xde-p\xe2\x97\x920\xa1\x89\xe3\xbcU\xa8\'\xd6K\xed\x90\xad\x8b\xfac%Y-\xa85\xdd\xca\x973\xbc\xe5\xcd\xc7\x9d\xd1\xec\xef^\x0eJ\x90\x06&c\xad\xb9\xd95-\x07\xbave,\xacW\x8f}\xf4\x07\x94\xd7\x81\x02\x96]\xa3\x07I\xd5z\xd0W\xf9\x1b\xe7SFu\xaa\xb0yB\xcbhq\x08\xe9`\xbd9i\xce\xf4\xaf\xc3V@\xc7\xadR\xa2\t\xe4o\x86G\x8a\x1f\xeb(\']\x83 \xaf\x04\xc9lV\x9a\x8bF\xf5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=GlobalSign Root CA O=GlobalSign nv-sa OU=Root CA
     * Subject: CN=GlobalSign Root CA O=GlobalSign nv-sa OU=Root CA
     * Label: "GlobalSign Root CA"
     * Serial: 4835703278459707669005204
     * MD5 Fingerprint: 3e:45:52:15:09:51:92:e1:b7:5d:37:9f:b1:87:29:8a
     * SHA1 Fingerprint: b1:bc:96:8b:d4:f4:9d:62:2a:a8:9a:81:f2:15:01:52:a4:1d:82:9c
     * SHA256 Fingerprint: eb:d4:10:40:e4:bb:3e:c7:42:c9:e3:81:d3:1e:f2:a4:1a:48:b6:68:5c:96:e7:ce:f3:c1:df:6c:d4:33:1c:99
     * -----BEGIN CERTIFICATE-----
     * MIIDdTCCAl2gAwIBAgILBAAAAAABFUtaw5QwDQYJKoZIhvcNAQEFBQAwVzELMAkG
     * A1UEBhMCQkUxGTAXBgNVBAoTEEdsb2JhbFNpZ24gbnYtc2ExEDAOBgNVBAsTB1Jv
     * b3QgQ0ExGzAZBgNVBAMTEkdsb2JhbFNpZ24gUm9vdCBDQTAeFw05ODA5MDExMjAw
     * MDBaFw0yODAxMjgxMjAwMDBaMFcxCzAJBgNVBAYTAkJFMRkwFwYDVQQKExBHbG9i
     * YWxTaWduIG52LXNhMRAwDgYDVQQLEwdSb290IENBMRswGQYDVQQDExJHbG9iYWxT
     * aWduIFJvb3QgQ0EwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDaDuaZ
     * jc6j40+Kfvvxi4Mla+pIH/EqsLmVEQS98GPR4mdmzxzdzxtIK+6NiY6arymAZavp
     * xy0Sy6scTHAHoT0KMM0VjU/43dSMUBUc71DuxC73/OlS8pF94G3VNTCOXkNz8kHp
     * 1Wrjsok6Vjk4bwY8iGlbKk3Fp1S4bInMm/k8yuX9ifUSPJJ4ltbcdG6TRGHRjcdG
     * snUOhugZitVtbNV4FpWi6cgKOOvyJBNPc1STE4U6G7weNLWLBYy5d4ux2x8gkasJ
     * U26Qzns3dLlwR5EiUWMWea6xrkEmCMgZK9FGqkjWZCrXgzT/LCrBbBlDSgeF59N8
     * 9iFo7+ryUp9/k5DPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNVHRMBAf8E
     * BTADAQH/MB0GA1UdDgQWBBRge2YaRQ2XyolQL30EzTSo//z9SzANBgkqhkiG9w0B
     * AQUFAAOCAQEA1nPnfE920I2/7LqivjTFKDK1fPxsnCwrvQmeU79rXqoRSLblCKOz
     * yj1hTdNGCbM+w6DjY1Ub8rrvrTnhQ7k4o+YviiY776BQVvnGCv04zcQLcFGUl5gE
     * 38NflNUVyRRBnMRddWQVDf9VMOyGj/8N7yy5Y0b2qvzfvGn9LhJIZJrglfCm7ymP
     * AbEVtQwdpf5pLGkkeB6zpxxxYu7KyJesF12KwvhHhm4qxFYxldBniYUr+WymXUad
     * DKqC5JlR3XC321Y9YeRq4VzW9v493kHMB65jUr9TU/Qr6cf9tveCX4XSQRjbgbME
     * HMUfpIBvFSDJ3gyICh3WZlXi/EjJKSZp4A==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02BE1\x190\x17\x06\x03U\x04\n\x13\x10GlobalSign nv-sa1\x100\x0e\x06\x03U\x04\x0b\x13\x07Root CA1\x1b0\x19\x06\x03U\x04\x03\x13\x12GlobalSign Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xda\x0e\xe6\x99\x8d\xce\xa3\xe3O\x8a~\xfb\xf1\x8b\x83%k\xeaH\x1f\xf1*\xb0\xb9\x95\x11\x04\xbd\xf0c\xd1\xe2gf\xcf\x1c\xdd\xcf\x1bH+\xee\x8d\x89\x8e\x9a\xaf)\x80e\xab\xe9\xc7-\x12\xcb\xab\x1cLp\x07\xa1=\n0\xcd\x15\x8dO\xf8\xdd\xd4\x8cP\x15\x1c\xefP\xee\xc4.\xf7\xfc\xe9R\xf2\x91}\xe0m\xd550\x8e^Cs\xf2A\xe9\xd5j\xe3\xb2\x89:V98o\x06<\x88i[*M\xc5\xa7T\xb8l\x89\xcc\x9b\xf9<\xca\xe5\xfd\x89\xf5\x12<\x92x\x96\xd6\xdctn\x93Da\xd1\x8d\xc7F\xb2u\x0e\x86\xe8\x19\x8a\xd5ml\xd5x\x16\x95\xa2\xe9\xc8\n8\xeb\xf2$\x13OsT\x93\x13\x85:\x1b\xbc\x1e4\xb5\x8b\x05\x8c\xb9w\x8b\xb1\xdb\x1f \x91\xab\tSn\x90\xce{7t\xb9pG\x91\"Qc\x16y\xae\xb1\xaeA&\x08\xc8\x19+\xd1F\xaaH\xd6d*\xd7\x834\xff,*\xc1l\x19CJ\x07\x85\xe7\xd3|\xf6!h\xef\xea\xf2R\x9f\x7f\x93\x90\xcf\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Buypass Class 3 Root CA O=Buypass AS-983163327
     * Subject: CN=Buypass Class 3 Root CA O=Buypass AS-983163327
     * Label: "Buypass Class 3 Root CA"
     * Serial: 2
     * MD5 Fingerprint: 3d:3b:18:9e:2c:64:5a:e8:d5:88:ce:0e:f9:37:c2:ec
     * SHA1 Fingerprint: da:fa:f7:fa:66:84:ec:06:8f:14:50:bd:c7:c2:81:a5:bc:a9:64:57
     * SHA256 Fingerprint: ed:f7:eb:bc:a2:7a:2a:38:4d:38:7b:7d:40:10:c6:66:e2:ed:b4:84:3e:4c:29:b4:ae:1d:5b:93:32:e6:b2:4d
     * -----BEGIN CERTIFICATE-----
     * MIIFWTCCA0GgAwIBAgIBAjANBgkqhkiG9w0BAQsFADBOMQswCQYDVQQGEwJOTzEd
     * MBsGA1UECgwUQnV5cGFzcyBBUy05ODMxNjMzMjcxIDAeBgNVBAMMF0J1eXBhc3Mg
     * Q2xhc3MgMyBSb290IENBMB4XDTEwMTAyNjA4Mjg1OFoXDTQwMTAyNjA4Mjg1OFow
     * TjELMAkGA1UEBhMCTk8xHTAbBgNVBAoMFEJ1eXBhc3MgQVMtOTgzMTYzMzI3MSAw
     * HgYDVQQDDBdCdXlwYXNzIENsYXNzIDMgUm9vdCBDQTCCAiIwDQYJKoZIhvcNAQEB
     * BQADggIPADCCAgoCggIBAKXaCpUWUOOV8l6ddjEGMnqb8RB2uACatVI2zSRHsJ8Y
     * ZLya9vrVediQYkwiL944PdbgqOkcLNt4EemOaFEVcsfzM4fkoF0LXOBXByow9c3E
     * N3coTRiR5r/VUv1xLXA+58bEiuPwKAv0dpihi4dVsjoT/Lc+JzeOIuOoTyrvYLs9
     * tznDDgFHmV0ST9tD+leh7fmdvhFHJlsTmKtdFoqwNxxXnUX/iJY2v7vKB3tvh2PX
     * 0DJq1l1sDPGzbjniazEuOQAnFN44wOwZZoYS6J1yFhNkUsepNxz9gjDthBgd9K5c
     * /3ATAOux9TN6S9ZV+AWNS2mw9bMoNlwUxFFzTWsL8TQH2xc519woe2v1n/MuwU8X
     * KhDzzMro6/1rqy6any2CbgTUUgGTLT2G/H783+9CHaZr77kgxve9oKeV/afmiSTY
     * zIw0bOIjL9kSGiG5VZFvC5F5GQytQIgLcOJ60g7YaEi7ghM5EFjp2CoHxhLbWNvS
     * O1UQRwUVZ2J+GGOmRj8JDlQyXr8NYnon74Do29lLBlo3WiXQCBJ31G8JUJc9yB3D
     * 34xFMFbG02SrZvPAXpacw8Tvw3xrizp5f7NJzz3iiZ+gMEuFuZyUJHmPfWupRWgP
     * K9Dx2hzLabjKSWJtyNBjYt1gD1iqj6G8BaVmos8bdrKEZLFMOVLAMLrwjEsCsLa3
     * AgMBAAGjQjBAMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFEe4zf/lb+74suwv
     * Tg75JbCOPGvDMA4GA1UdDwEB/wQEAwIBBjANBgkqhkiG9w0BAQsFAAOCAgEAACAj
     * QTUEkMJAYmDv4jVM1z+s4jSQuKFvdvoWFqRINyzpkMLyPPgKn9iB5btb2iUspKdV
     * cSQy9sgL8rxq+JOssgfCX5/bzMiKqr5qb+FJEMwx14C7u8jYog5kV+qi9cKpMRXS
     * IGrs/CIBKM+GuIAeqcwRpTzyFrNHnfzSgCHEy9BHcEGhyoMZCCxt8l13nIoUE9Q2
     * HJLw5QY33KbmkJs4j1xrG0aGQ0JfPgEHU1RdZX33inOhmlRaHylDFCfChQ+1iHsa
     * O5S3HWCntZznKWlXWpuTekMwGwPXYshApqr8ZORK15FTAaggiG6cX0S5y2CBNOxv
     * 033aSF/rtJC8LakcC6wc1aJoIIAE1vyxjy+7SjENSoYc6+I2KSb12tjE8nVhz36u
     * dmNKekBlk4f4HoCMhuWG1o8O/FMsYOgWYRqiPkN7zTlgVGr18okmAWiDSKIz6MkE
     * kbIRNBE+6tBDGR8Dk5AM/1E9V/RBbuHLoL7ryWPNbczk+DaqaJ3tvV2XcEQNtg41
     * 3OEMXbugUZTLfhbrES+jkkXITHHZvMmZUldGL1DPvTVp9D0VzgalLA8+9oG6lLvD
     * u79leNKGef9JOxqDDPDeeOzI8k1MGt6CKfjBWtrt7uYnXuhF0J0cUahoq0Tj0Itq
     * 4/g7u9xN12TyUb7mqqta6THuBrxzvxNiCp/HuZc=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02NO1\x1d0\x1b\x06\x03U\x04\n\x0c\x14Buypass AS-9831633271 0\x1e\x06\x03U\x04\x03\x0c\x17Buypass Class 3 Root CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x02\x0f\x000\x82\x02\n\x02\x82\x02\x01\x00\xa5\xda\n\x95\x16P\xe3\x95\xf2^\x9dv1\x062z\x9b\xf1\x10v\xb8\x00\x9a\xb5R6\xcd$G\xb0\x9f\x18d\xbc\x9a\xf6\xfa\xd5y\xd8\x90bL\"/\xde8=\xd6\xe0\xa8\xe9\x1c,\xdbx\x11\xe9\x8ehQ\x15r\xc7\xf33\x87\xe4\xa0]\x0b\\\xe0W\x07*0\xf5\xcd\xc47w(M\x18\x91\xe6\xbf\xd5R\xfdq-p>\xe7\xc6\xc4\x8a\xe3\xf0(\x0b\xf4v\x98\xa1\x8b\x87U\xb2:\x13\xfc\xb7>\'7\x8e\"\xe3\xa8O*\xef`\xbb=\xb79\xc3\x0e\x01G\x99]\x12O\xdbC\xfaW\xa1\xed\xf9\x9d\xbe\x11G&[\x13\x98\xab]\x16\x8a\xb07\x1cW\x9dE\xff\x88\x966\xbf\xbb\xca\x07{o\x87c\xd7\xd02j\xd6]l\x0c\xf1\xb3n9\xe2k1.9\x00\'\x14\xde8\xc0\xec\x19f\x86\x12\xe8\x9dr\x16\x13dR\xc7\xa97\x1c\xfd\x820\xed\x84\x18\x1d\xf4\xae\\\xffp\x13\x00\xeb\xb1\xf53zK\xd6U\xf8\x05\x8dKi\xb0\xf5\xb3(6\\\x14\xc4QsMk\x0b\xf14\x07\xdb\x179\xd7\xdc({k\xf5\x9f\xf3.\xc1O\x17*\x10\xf3\xcc\xca\xe8\xeb\xfdk\xab.\x9a\x9f-\x82n\x04\xd4R\x01\x93-=\x86\xfc~\xfc\xdf\xefB\x1d\xa6k\xef\xb9 \xc6\xf7\xbd\xa0\xa7\x95\xfd\xa7\xe6\x89$\xd8\xcc\x8c4l\xe2#/\xd9\x12\x1a!\xb9U\x91o\x0b\x91y\x19\x0c\xad@\x88\x0bp\xe2z\xd2\x0e\xd8hH\xbb\x82\x139\x10X\xe9\xd8*\x07\xc6\x12\xdbX\xdb\xd2;U\x10G\x05\x15gb~\x18c\xa6F?\t\x0eT2^\xbf\rbz\'\xef\x80\xe8\xdb\xd9K\x06Z7Z%\xd0\x08\x12w\xd4o\tP\x97=\xc8\x1d\xc3\xdf\x8cE0V\xc6\xd3d\xabf\xf3\xc0^\x96\x9c\xc3\xc4\xef\xc3|k\x8b:y\x7f\xb3I\xcf=\xe2\x89\x9f\xa00K\x85\xb9\x9c\x94$y\x8f}k\xa9Eh\x0f+\xd0\xf1\xda\x1c\xcbi\xb8\xcaIbm\xc8\xd0cb\xdd`\x0fX\xaa\x8f\xa1\xbc\x05\xa5f\xa2\xcf\x1bv\xb2\x84d\xb1L9R\xc00\xba\xf0\x8cK\x02\xb0\xb6\xb7\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=D-TRUST Root Class 3 CA 2 EV 2009 O=D-Trust GmbH
     * Subject: CN=D-TRUST Root Class 3 CA 2 EV 2009 O=D-Trust GmbH
     * Label: "D-TRUST Root Class 3 CA 2 EV 2009"
     * Serial: 623604
     * MD5 Fingerprint: aa:c6:43:2c:5e:2d:cd:c4:34:c0:50:4f:11:02:4f:b6
     * SHA1 Fingerprint: 96:c9:1b:0b:95:b4:10:98:42:fa:d0:d8:22:79:fe:60:fa:b9:16:83
     * SHA256 Fingerprint: ee:c5:49:6b:98:8c:e9:86:25:b9:34:09:2e:ec:29:08:be:d0:b0:f3:16:c2:d4:73:0c:84:ea:f1:f3:d3:48:81
     * -----BEGIN CERTIFICATE-----
     * MIIEQzCCAyugAwIBAgIDCYP0MA0GCSqGSIb3DQEBCwUAMFAxCzAJBgNVBAYTAkRF
     * MRUwEwYDVQQKDAxELVRydXN0IEdtYkgxKjAoBgNVBAMMIUQtVFJVU1QgUm9vdCBD
     * bGFzcyAzIENBIDIgRVYgMjAwOTAeFw0wOTExMDUwODUwNDZaFw0yOTExMDUwODUw
     * NDZaMFAxCzAJBgNVBAYTAkRFMRUwEwYDVQQKDAxELVRydXN0IEdtYkgxKjAoBgNV
     * BAMMIUQtVFJVU1QgUm9vdCBDbGFzcyAzIENBIDIgRVYgMjAwOTCCASIwDQYJKoZI
     * hvcNAQEBBQADggEPADCCAQoCggEBAJnxhDRwui+3MKCOvXwEz75ivJn9gpfSegpn
     * ljgJ9hBOlSJzmY3aFS3nBfwZcyK3jpgAvDw9rKFs+9Z5JUut8Mxk2og+KbgPCdM0
     * 3TP1YtHhzRnp7hhPTFiu4h7WDFsVWtg6uMQYZB7jM7K1iXdODL/ZlGsTl28So/6Z
     * qQTMFexgaDbtCHu39b+T7WYxg4zGcTSHThfqr4uRjRxWQa4iN1438h3Z0S0NL2lR
     * p75mpoo6Kr3HGrHhFPC+Oh25z1uxav60sUYgovseO3Dvk5h9jHOW8sXvhXCtKSb8
     * HgQ+HKDYD8tSg2J87otTlZCpV6LqYQXY+U3EJ/pure3511H3a6UCAwEAAaOCASQw
     * ggEgMA8GA1UdEwEB/wQFMAMBAf8wHQYDVR0OBBYEFNOUikxiEyoZLsyvcop9Ntea
     * HNxnMA4GA1UdDwEB/wQEAwIBBjCB3QYDVR0fBIHVMIHSMIGHoIGEoIGBhn9sZGFw
     * Oi8vZGlyZWN0b3J5LmQtdHJ1c3QubmV0L0NOPUQtVFJVU1QlMjBSb290JTIwQ2xh
     * c3MlMjAzJTIwQ0ElMjAyJTIwRVYlMjAyMDA5LE89RC1UcnVzdCUyMEdtYkgsQz1E
     * RT9jZXJ0aWZpY2F0ZXJldm9jYXRpb25saXN0MEagRKBChkBodHRwOi8vd3d3LmQt
     * dHJ1c3QubmV0L2NybC9kLXRydXN0X3Jvb3RfY2xhc3NfM19jYV8yX2V2XzIwMDku
     * Y3JsMA0GCSqGSIb3DQEBCwUAA4IBAQA07XtaPKSUiO8aEXUHL7P+PPoeUSbrh/Yp
     * 3uDx1MYkCenBz1UbtDDZzhr+BlGmFaQt77JLvyAoJUnRpjZ3NOhk31KxEcdzes05
     * nsKtjHEh8lprr988TlWvsoRlFIm5d8sqMb7Po23Pb0iUMkZv53GMoKaEGTcH8gNF
     * CSuGdXzfX2lXANtu2KZyIktQ1HWYVt+3GP9DQ1CuekR78HlR10M9p9OB0/DJT7na
     * xpeG0ILD5EJt/rDiZE4OJudANCa1CInXCGNjOCd1HjPqbqjdn5lPdE2BiYBL3ZqX
     * KVwvvoFBuYz/6n1gBp7N1z3TLqMVvKjmJuVvw9y4AyHqnxbxLFS1
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02DE1\x150\x13\x06\x03U\x04\n\x0c\x0cD-Trust GmbH1*0(\x06\x03U\x04\x03\x0c!D-TRUST Root Class 3 CA 2 EV 2009",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\x99\xf1\x844p\xba/\xb70\xa0\x8e\xbd|\x04\xcf\xbeb\xbc\x99\xfd\x82\x97\xd2z\ng\x968\t\xf6\x10N\x95\"s\x99\x8d\xda\x15-\xe7\x05\xfc\x19s\"\xb7\x8e\x98\x00\xbc<=\xac\xa1l\xfb\xd6y%K\xad\xf0\xccd\xda\x88>)\xb8\x0f\t\xd34\xdd3\xf5b\xd1\xe1\xcd\x19\xe9\xee\x18OLX\xae\xe2\x1e\xd6\x0c[\x15Z\xd8:\xb8\xc4\x18d\x1e\xe33\xb2\xb5\x89wN\x0c\xbf\xd9\x94k\x13\x97o\x12\xa3\xfe\x99\xa9\x04\xcc\x15\xec`h6\xed\x08{\xb7\xf5\xbf\x93\xedf1\x83\x8c\xc6q4\x87N\x17\xea\xaf\x8b\x91\x8d\x1cVA\xae\"7^7\xf2\x1d\xd9\xd1-\r/iQ\xa7\xbef\xa6\x8a:*\xbd\xc7\x1a\xb1\xe1\x14\xf0\xbe:\x1d\xb9\xcf[\xb1j\xfe\xb4\xb1F \xa2\xfb\x1e;p\xef\x93\x98}\x8cs\x96\xf2\xc5\xef\x85p\xad)&\xfc\x1e\x04>\x1c\xa0\xd8\x0f\xcbR\x83b|\xee\x8bS\x95\x90\xa9W\xa2\xeaa\x05\xd8\xf9M\xc4\'\xfan\xad\xed\xf9\xd7Q\xf7k\xa5\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=SecureTrust CA O=SecureTrust Corporation
     * Subject: CN=SecureTrust CA O=SecureTrust Corporation
     * Label: "SecureTrust CA"
     * Serial: 17199774589125277788362757014266862032
     * MD5 Fingerprint: dc:32:c3:a7:6d:25:57:c7:68:09:9d:ea:2d:a9:a2:d1
     * SHA1 Fingerprint: 87:82:c6:c3:04:35:3b:cf:d2:96:92:d2:59:3e:7d:44:d9:34:ff:11
     * SHA256 Fingerprint: f1:c1:b5:0a:e5:a2:0d:d8:03:0e:c9:f6:bc:24:82:3d:d3:67:b5:25:57:59:b4:e7:1b:61:fc:e9:f7:37:5d:73
     * -----BEGIN CERTIFICATE-----
     * MIIDuDCCAqCgAwIBAgIQDPCOXAgWpa1Cf/DrJxhZ0DANBgkqhkiG9w0BAQUFADBI
     * MQswCQYDVQQGEwJVUzEgMB4GA1UEChMXU2VjdXJlVHJ1c3QgQ29ycG9yYXRpb24x
     * FzAVBgNVBAMTDlNlY3VyZVRydXN0IENBMB4XDTA2MTEwNzE5MzExOFoXDTI5MTIz
     * MTE5NDA1NVowSDELMAkGA1UEBhMCVVMxIDAeBgNVBAoTF1NlY3VyZVRydXN0IENv
     * cnBvcmF0aW9uMRcwFQYDVQQDEw5TZWN1cmVUcnVzdCBDQTCCASIwDQYJKoZIhvcN
     * AQEBBQADggEPADCCAQoCggEBAKukgeWVzfX2FI7CT8rU4niVWJxB4Q2ZQCQXOZEz
     * Zum+4YOvYlyJ0fwkW2Gz4BERQRwdbvC4u/jep4G6pkjGnx29vo6pQT64lO0pGtSO
     * 0gMdA+9tDWccV9cGrcrI9f4Or2YlSASWC12juhbDCE/RRvgUXPLIXgGZbf2IzIao
     * wW8xQmxSPmjL8xk037uHGFaAJsTQ3MBv396gwpEWoGQRS0S8Hvbn+mPeZqx2pHGj
     * 7DaUaHp3pLHnDi+BeuK1cobvomuL8A/b01k/unK8RCSc43Oz969XL0Imnal0ugBS
     * 8kvNU3xHCzaFDmapCJcWNFfBZveA4+1wVMeT4C4oFVmHursCAwEAAaOBnTCBmjAT
     * BgkrBgEEAYI3FAIEBh4EAEMAQTALBgNVHQ8EBAMCAYYwDwYDVR0TAQH/BAUwAwEB
     * /zAdBgNVHQ4EFgQUQjK2FvoE/f5dS3rD/fdMQB1aQ68wNAYDVR0fBC0wKzApoCeg
     * JYYjaHR0cDovL2NybC5zZWN1cmV0cnVzdC5jb20vU1RDQS5jcmwwEAYJKwYBBAGC
     * NxUBBAMCAQAwDQYJKoZIhvcNAQEFBQADggEBADDtT0rhWDpSclu1pqNlGKa7UTt3
     * 6Z3q059c4EVlew3KW+JwULKUBRSuSceNQQcSc5R+DCMh/bwQf2AQWnL1mA6s7Ll/
     * 3XpvXdMc9P+IBWlCqQVxyLesJugutIxq/3HcuLHfmbx8IVQr5Fiiu1cprp6poxkm
     * D5kuCLDv/WnPmRoJjeOnnyvJNjR7JLN4TJUXpAYmHrZkUjZfYGfZnMUFdAvnZyPS
     * CPyI6a6Lf+Ew9Dd+/cYy2i2eRDAwbO4H3tI0/NL/QPZL9GZGBlSm8jIKYyYwa5vR
     * 3ItHuuG51WLQoqD0ZwV4KWMabwTW+MZMo5qxN7SN5ShLHZ4swrhovO0C7jE=
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02US1 0\x1e\x06\x03U\x04\n\x13\x17SecureTrust Corporation1\x170\x15\x06\x03U\x04\x03\x13\x0eSecureTrust CA",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xab\xa4\x81\xe5\x95\xcd\xf5\xf6\x14\x8e\xc2O\xca\xd4\xe2x\x95X\x9cA\xe1\r\x99@$\x179\x913f\xe9\xbe\xe1\x83\xafb\\\x89\xd1\xfc$[a\xb3\xe0\x11\x11A\x1c\x1dn\xf0\xb8\xbb\xf8\xde\xa7\x81\xba\xa6H\xc6\x9f\x1d\xbd\xbe\x8e\xa9A>\xb8\x94\xed)\x1a\xd4\x8e\xd2\x03\x1d\x03\xefm\rg\x1cW\xd7\x06\xad\xca\xc8\xf5\xfe\x0e\xaff%H\x04\x96\x0b]\xa3\xba\x16\xc3\x08O\xd1F\xf8\x14\\\xf2\xc8^\x01\x99m\xfd\x88\xcc\x86\xa8\xc1o1BlR>h\xcb\xf3\x194\xdf\xbb\x87\x18V\x80&\xc4\xd0\xdc\xc0o\xdf\xde\xa0\xc2\x91\x16\xa0d\x11KD\xbc\x1e\xf6\xe7\xfac\xdef\xacv\xa4q\xa3\xec6\x94hzw\xa4\xb1\xe7\x0e/\x81z\xe2\xb5r\x86\xef\xa2k\x8b\xf0\x0f\xdb\xd3Y?\xbar\xbcD$\x9c\xe3s\xb3\xf7\xafW/B&\x9d\xa9t\xba\x00R\xf2K\xcdS|G\x0b6\x85\x0ef\xa9\x08\x97\x164W\xc1f\xf7\x80\xe3\xedpT\xc7\x93\xe0.(\x15Y\x87\xba\xbb\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Atos TrustedRoot 2011 O=Atos
     * Subject: CN=Atos TrustedRoot 2011 O=Atos
     * Label: "Atos TrustedRoot 2011"
     * Serial: 6643877497813316402
     * MD5 Fingerprint: ae:b9:c4:32:4b:ac:7f:5d:66:cc:77:94:bb:2a:77:56
     * SHA1 Fingerprint: 2b:b1:f5:3e:55:0c:1d:c5:f1:d4:e6:b7:6a:46:4b:55:06:02:ac:21
     * SHA256 Fingerprint: f3:56:be:a2:44:b7:a9:1e:b3:5d:53:ca:9a:d7:86:4a:ce:01:8e:2d:35:d5:f8:f9:6d:df:68:a6:f4:1a:a4:74
     * -----BEGIN CERTIFICATE-----
     * MIIDdzCCAl+gAwIBAgIIXDPLYixfszIwDQYJKoZIhvcNAQELBQAwPDEeMBwGA1UE
     * AwwVQXRvcyBUcnVzdGVkUm9vdCAyMDExMQ0wCwYDVQQKDARBdG9zMQswCQYDVQQG
     * EwJERTAeFw0xMTA3MDcxNDU4MzBaFw0zMDEyMzEyMzU5NTlaMDwxHjAcBgNVBAMM
     * FUF0b3MgVHJ1c3RlZFJvb3QgMjAxMTENMAsGA1UECgwEQXRvczELMAkGA1UEBhMC
     * REUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCVhTuXbyo7LjvPpvMp
     * Nb7PGKw+qtn4TaA+Gke5vJrf8v7MPkfoepbCJI419KkM/IL9bcFyYie96mvr54rM
     * VD6QUM+A1JX76LWC1BTFtqlVJVfbsVD2sGBkWXppzwO3bw2+yj5vdHLqqjAqc2K+
     * SZFhyBH+DgMq92og3AIVDV4VavzjgsG1xZ1kCWyjWZgHJ8cblithdHFsQ/H3NYkQ
     * 4J7sVaE3IqKHBAUsR320HLliKWYoyrfhk/WklAOZuXCFteZI6o1Q/NnezG8HDt0L
     * cp2AMBYHlT8oDv3FdU9T1nSatCQujgKRz3bFmx5VdJx4IbHwLfELn8LVlhgf8FQi
     * eowHAgMBAAGjfTB7MB0GA1UdDgQWBBSnpQaxLKYJYO7Rl+lwrrw7GWzbITAPBgNV
     * HRMBAf8EBTADAQH/MB8GA1UdIwQYMBaAFKelBrEspglg7tGX6XCuvDsZbNshMBgG
     * A1UdIAQRMA8wDQYLKwYBBAGwLQMEAQEwDgYDVR0PAQH/BAQDAgGGMA0GCSqGSIb3
     * DQEBCwUAA4IBAQAmdzTblEiGKkGdLD4GkGDEjKwLVLgfuXvTBznk+j57sj1O7Z8j
     * vZfza1zv7v1Apt+hk6EKhqzvINB5Ab149xnYJDE0BAGmuhWawyfc2E8PzBhj/5kP
     * DpFrdRbhIfzYJsdHt6bPWHJxfrrhTZVHO8mvbaG0weyJ9rQPOLXiZNwlz6bb65pc
     * maHFCN795trV1lpFDMS3wrUU77QR/w4VtfX128a961qn8FYiqTxlVMYVqL2Gns2D
     * lmh6cYGJ4Qvh6hEbaAjMaZ7snkGeRDImeuKHCnE96+RapNLbxc3G3mB/ufNPRJLv
     * KrcYPqcZ2Qt9sTdBQrC6YB3y/gkRsPCHe6ed
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x1e0\x1c\x06\x03U\x04\x03\x0c\x15Atos TrustedRoot 20111\r0\x0b\x06\x03U\x04\n\x0c\x04Atos1\x0b0\t\x06\x03U\x04\x06\x13\x02DE",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\x95\x85;\x97o*;.;\xcf\xa6\xf3)5\xbe\xcf\x18\xac>\xaa\xd9\xf8M\xa0>\x1aG\xb9\xbc\x9a\xdf\xf2\xfe\xcc>G\xe8z\x96\xc2$\x8e5\xf4\xa9\x0c\xfc\x82\xfdm\xc1rb\'\xbd\xeak\xeb\xe7\x8a\xccT>\x90P\xcf\x80\xd4\x95\xfb\xe8\xb5\x82\xd4\x14\xc5\xb6\xa9U%W\xdb\xb1P\xf6\xb0`dYzi\xcf\x03\xb7o\r\xbe\xca>otr\xea\xaa0*sb\xbeI\x91a\xc8\x11\xfe\x0e\x03*\xf7j \xdc\x02\x15\r^\x15j\xfc\xe3\x82\xc1\xb5\xc5\x9dd\tl\xa3Y\x98\x07\'\xc7\x1b\x96+atqlC\xf1\xf75\x89\x10\xe0\x9e\xecU\xa17\"\xa2\x87\x04\x05,G}\xb4\x1c\xb9b)f(\xca\xb7\xe1\x93\xf5\xa4\x94\x03\x99\xb9p\x85\xb5\xe6H\xea\x8dP\xfc\xd9\xde\xcco\x07\x0e\xdd\x0br\x9d\x800\x16\x07\x95?(\x0e\xfd\xc5uOS\xd6t\x9a\xb4$.\x8e\x02\x91\xcfv\xc5\x9b\x1eUt\x9cx!\xb1\xf0-\xf1\x0b\x9f\xc2\xd5\x96\x18\x1f\xf0T\"z\x8c\x07\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=Hongkong Post Root CA 1 O=Hongkong Post
     * Subject: CN=Hongkong Post Root CA 1 O=Hongkong Post
     * Label: "Hongkong Post Root CA 1"
     * Serial: 1000
     * MD5 Fingerprint: a8:0d:6f:39:78:b9:43:6d:77:42:6d:98:5a:cc:23:ca
     * SHA1 Fingerprint: d6:da:a8:20:8d:09:d2:15:4d:24:b5:2f:cb:34:6e:b2:58:b2:8a:58
     * SHA256 Fingerprint: f9:e6:7d:33:6c:51:00:2a:c0:54:c6:32:02:2d:66:dd:a2:e7:e3:ff:f1:0a:d0:61:ed:31:d8:bb:b4:10:cf:b2
     * -----BEGIN CERTIFICATE-----
     * MIIDMDCCAhigAwIBAgICA+gwDQYJKoZIhvcNAQEFBQAwRzELMAkGA1UEBhMCSEsx
     * FjAUBgNVBAoTDUhvbmdrb25nIFBvc3QxIDAeBgNVBAMTF0hvbmdrb25nIFBvc3Qg
     * Um9vdCBDQSAxMB4XDTAzMDUxNTA1MTMxNFoXDTIzMDUxNTA0NTIyOVowRzELMAkG
     * A1UEBhMCSEsxFjAUBgNVBAoTDUhvbmdrb25nIFBvc3QxIDAeBgNVBAMTF0hvbmdr
     * b25nIFBvc3QgUm9vdCBDQSAxMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKC
     * AQEArP84tulmAknjorThkPlAj3n54r15/gK97iSSHSL22oVyaf7XPwnU3ZG1ApzQ
     * jVrhVcNQhrkpJsLj2aDxaQMoIIBFIi1WpztUlVYiWR8o3x8gPW2iNr4joLFutbEn
     * PzlTCeqrauh0ssJlXI6/fMN4hM2eFvz1Lk8gKgifd/PFHsSaUmYeSF7jEAaPIpjh
     * ZY4bXSNmO7ilMlHIhqqhqZ5/dpTCpmy3QfDVyAY45tQM4vM7TG1QjMSDJ8EThFk9
     * nnV0ttgCXjqQesBCNnLsak3c78QA3xMYV18meMjWCnl3v/evt3a5pQuEF10Q6m/h
     * q5URX208o1xNg1vysxmKgIsLhwIDAQABoyYwJDASBgNVHRMBAf8ECDAGAQH/AgED
     * MA4GA1UdDwEB/wQEAwIBxjANBgkqhkiG9w0BAQUFAAOCAQEADkbVPK7ih9legYsC
     * mEEIjEy82tvuJxuC52pF7BaLT4Wg87JwvVqWuspube5Gi27nKi6Wsxkz67SfqLI3
     * 7piol7Yutmcn1KZJ/RyTZXaeQi/cImyaT/JaFTmxcdcrUehtHJjA2Sr0oYJ71clB
     * oiMBdDhViw+5LmeiIAQ32pwL0xch4I+XeTRvhEgCIDMb5jREn5Fw9IBehEPCKdJs
     * EhTkYY2sEJCehFC78JZvRZ+K88psT/oROhUVRsPNH4NbLUES7VBnQRM9IauUiqpO
     * fMGx+6fWtScvl6tu4B3i0RwsH0Ti/L6RoZz71ilTc4afU9hDDl3WY4JxHYB0yvbi
     * AmvZWg==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02HK1\x160\x14\x06\x03U\x04\n\x13\rHongkong Post1 0\x1e\x06\x03U\x04\x03\x13\x17Hongkong Post Root CA 1",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xac\xff8\xb6\xe9f\x02I\xe3\xa2\xb4\xe1\x90\xf9@\x8fy\xf9\xe2\xbdy\xfe\x02\xbd\xee$\x92\x1d\"\xf6\xda\x85ri\xfe\xd7?\t\xd4\xdd\x91\xb5\x02\x9c\xd0\x8dZ\xe1U\xc3P\x86\xb9)&\xc2\xe3\xd9\xa0\xf1i\x03( \x80E\"-V\xa7;T\x95V\"Y\x1f(\xdf\x1f =m\xa26\xbe#\xa0\xb1n\xb5\xb1\'?9S\t\xea\xabj\xe8t\xb2\xc2e\\\x8e\xbf|\xc3x\x84\xcd\x9e\x16\xfc\xf5.O *\x08\x9fw\xf3\xc5\x1e\xc4\x9aRf\x1eH^\xe3\x10\x06\x8f\"\x98\xe1e\x8e\x1b]#f;\xb8\xa52Q\xc8\x86\xaa\xa1\xa9\x9e\x7fv\x94\xc2\xa6l\xb7A\xf0\xd5\xc8\x068\xe6\xd4\x0c\xe2\xf3;LmP\x8c\xc4\x83\'\xc1\x13\x84Y=\x9eut\xb6\xd8\x02^:\x90z\xc0B6r\xecjM\xdc\xef\xc4\x00\xdf\x13\x18W_&x\xc8\xd6\nyw\xbf\xf7\xaf\xb7v\xb9\xa5\x0b\x84\x17]\x10\xeao\xe1\xab\x95\x11_m<\xa3\\M\x83[\xf2\xb3\x19\x8a\x80\x8b\x0b\x87\x02\x03\x01\x00\x01",
        name_constraints: None
    },

    /*
     * Issuer: CN=T-TeleSec GlobalRoot Class 3 O=T-Systems Enterprise Services GmbH OU=T-Systems Trust Center
     * Subject: CN=T-TeleSec GlobalRoot Class 3 O=T-Systems Enterprise Services GmbH OU=T-Systems Trust Center
     * Label: "T-TeleSec GlobalRoot Class 3"
     * Serial: 1
     * MD5 Fingerprint: ca:fb:40:a8:4e:39:92:8a:1d:fe:8e:2f:c4:27:ea:ef
     * SHA1 Fingerprint: 55:a6:72:3e:cb:f2:ec:cd:c3:23:74:70:19:9d:2a:be:11:e3:81:d1
     * SHA256 Fingerprint: fd:73:da:d3:1c:64:4f:f1:b4:3b:ef:0c:cd:da:96:71:0b:9c:d9:87:5e:ca:7e:31:70:7a:f3:e9:6d:52:2b:bd
     * -----BEGIN CERTIFICATE-----
     * MIIDwzCCAqugAwIBAgIBATANBgkqhkiG9w0BAQsFADCBgjELMAkGA1UEBhMCREUx
     * KzApBgNVBAoMIlQtU3lzdGVtcyBFbnRlcnByaXNlIFNlcnZpY2VzIEdtYkgxHzAd
     * BgNVBAsMFlQtU3lzdGVtcyBUcnVzdCBDZW50ZXIxJTAjBgNVBAMMHFQtVGVsZVNl
     * YyBHbG9iYWxSb290IENsYXNzIDMwHhcNMDgxMDAxMTAyOTU2WhcNMzMxMDAxMjM1
     * OTU5WjCBgjELMAkGA1UEBhMCREUxKzApBgNVBAoMIlQtU3lzdGVtcyBFbnRlcnBy
     * aXNlIFNlcnZpY2VzIEdtYkgxHzAdBgNVBAsMFlQtU3lzdGVtcyBUcnVzdCBDZW50
     * ZXIxJTAjBgNVBAMMHFQtVGVsZVNlYyBHbG9iYWxSb290IENsYXNzIDMwggEiMA0G
     * CSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC9dZPwYiJvJK7genasfb3ZJNW4t/zN
     * 8ELg63iIVl6bmlQdTQyK9tPPcPRStdiTBONGhnFBSivwKixVA9ZIw+A5OO3yXDw/
     * RLyTPWGrTs0NvvAgJ1gORH8EGoel15YUNpDQSXuhdfsaa3Ox+M6pCSzyU9XDFES4
     * hqX2iys52qMzVNn6chr3IhUciJFrf2blw2qAsCTz34ZFiP0Zf3WHHx+xGwpzJFu5
     * ZeAsVMhg02YXP+HMVDNzkQI6pn97djmiH5a2OK61yJN0HZ65tOVgnS9W0eDrXltM
     * EnAMbEQgqxHY9Bn20pxSN+f6tsIxO0rUFJmtxxr1XV/6B7h8DR/Wgx6zAgMBAAGj
     * QjBAMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgEGMB0GA1UdDgQWBBS1
     * A/d2O2GCahKqGFPrAyGUv/7OyjANBgkqhkiG9w0BAQsFAAOCAQEAVj3vlNW92nOy
     * WL6ukK2YJ5f+AbGwUgC4TeQbIXQbfsDuXmkqJa9c1h3a0nnJ85cp4IaH3gRZD/FZ
     * 1GSFS5mvJQQeyUapl96Cshtwn5z2r3Ex3XsFpSzTucpH9sry9uetuUg/vBa3wW30
     * 6gmv7PO15wWeph6KU1HWk4HMdJP2udqmJQV0eVp+QD6CSyYRMG7hP0HHRwA11fXT
     * 91Q+gT3aSWqas+8QPebrb9HIIkfLzM8BMZLZGOMivgkeGj5asuRrDFR6fUNOuIml
     * e9eiPZaGzPImNC1qkp2aGtAw4l1OBLBfiyB+d8E9lYLRRpo7PHi4b6HQDWSieB4p
     * TpPDpFQUWw==
     * -----END CERTIFICATE-----
     */
    webpki::TrustAnchor {
        subject: b"1\x0b0\t\x06\x03U\x04\x06\x13\x02DE1+0)\x06\x03U\x04\n\x0c\"T-Systems Enterprise Services GmbH1\x1f0\x1d\x06\x03U\x04\x0b\x0c\x16T-Systems Trust Center1%0#\x06\x03U\x04\x03\x0c\x1cT-TeleSec GlobalRoot Class 3",
        spki: b"0\r\x06\t*\x86H\x86\xf7\r\x01\x01\x01\x05\x00\x03\x82\x01\x0f\x000\x82\x01\n\x02\x82\x01\x01\x00\xbdu\x93\xf0b\"o$\xae\xe0zv\xac}\xbd\xd9$\xd5\xb8\xb7\xfc\xcd\xf0B\xe0\xebx\x88V^\x9b\x9aT\x1dM\x0c\x8a\xf6\xd3\xcfp\xf4R\xb5\xd8\x93\x04\xe3F\x86qAJ+\xf0*,U\x03\xd6H\xc3\xe098\xed\xf2\\<?D\xbc\x93=a\xabN\xcd\r\xbe\xf0 \'X\x0eD\x7f\x04\x1a\x87\xa5\xd7\x96\x146\x90\xd0I{\xa1u\xfb\x1aks\xb1\xf8\xce\xa9\t,\xf2S\xd5\xc3\x14D\xb8\x86\xa5\xf6\x8b+9\xda\xa33T\xd9\xfar\x1a\xf7\"\x15\x1c\x88\x91k\x7ff\xe5\xc3j\x80\xb0$\xf3\xdf\x86E\x88\xfd\x19\x7fu\x87\x1f\x1f\xb1\x1b\ns$[\xb9e\xe0,T\xc8`\xd3f\x17?\xe1\xccT3s\x91\x02:\xa6\x7f{v9\xa2\x1f\x96\xb68\xae\xb5\xc8\x93t\x1d\x9e\xb9\xb4\xe5`\x9d/V\xd1\xe0\xeb^[L\x12p\x0clD \xab\x11\xd8\xf4\x19\xf6\xd2\x9cR7\xe7\xfa\xb6\xc21;J\xd4\x14\x99\xad\xc7\x1a\xf5]_\xfa\x07\xb8|\r\x1f\xd6\x83\x1e\xb3\x02\x03\x01\x00\x01",
        name_constraints: None
    },

]);
