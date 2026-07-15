# Configuration

`dyndo-server` is configured by layering three sources, each overriding the one
before it:

1. **built-in defaults**,
2. a **`config.yaml`** file, then
3. **`DYNDO_`-prefixed environment variables**.

There are no command-line flags. Configuration is deserialized directly into
[OpenDAL](https://opendal.apache.org/)'s backend config structs, so backend
settings are exactly OpenDAL's own.

## Configuration file

The file is `./config.yaml` in the working directory by default. Set
`DYNDO_CONFIG` to load a different path:

| Variable | Description |
|---|---|
| `DYNDO_CONFIG` | Path to the YAML config file. If set, the file **must exist** or the server exits. If unset, a missing `config.yaml` is ignored (defaults + env are used). |

## Schema

| Key | Type | Description | Default |
|---|---|---|---|
| `store` | `fs` \| `s3` | Which storage backend serves assets. | `fs` |
| `server.host` | string | Listen address. | `0.0.0.0` |
| `server.port` | integer | Listen port. | `8080` |
| `fs` | table | Local-filesystem backend settings (OpenDAL `Fs`). Required when `store: fs`. | *(none)* |
| `s3` | table | S3 backend settings (OpenDAL `S3`). Required when `store: s3`. | *(none)* |

Neither backend section is defaulted: whichever backend `store` selects must be
supplied (via file or environment) with the fields OpenDAL needs, or the server
fails to build its storage operator at startup.

### `fs` backend

| Key | Description |
|---|---|
| `fs.root` | Root directory that descriptors and media are read from. **Required** for `store: fs`. |

```yaml
store: fs
fs:
  root: ./assets
```

### `s3` backend

The `s3` table is OpenDAL's S3 configuration. The commonly used keys:

| Key | Description |
|---|---|
| `s3.bucket` | Bucket name. **Required** for `store: s3`. |
| `s3.region` | AWS region. |
| `s3.endpoint` | Service endpoint URL. |
| `s3.root` | Key prefix prepended to every object path. |
| `s3.access_key_id` | Access key ID (or supply via `AWS_ACCESS_KEY_ID`). |
| `s3.secret_access_key` | Secret access key (or supply via `AWS_SECRET_ACCESS_KEY`). |

```yaml
store: s3
s3:
  bucket: my-assets
  region: eu-west-1
  endpoint: https://s3.eu-west-1.amazonaws.com
  root: /
```

Credentials may be omitted from the file and supplied through the standard
`AWS_*` environment variables, which OpenDAL's S3 backend reads.

## Environment variables

Every setting maps to a `DYNDO_`-prefixed variable. Nested keys are separated by
a **double underscore** (`__`); single underscores within a field name are
preserved.

| Variable | Sets |
|---|---|
| `DYNDO_STORE` | `store` |
| `DYNDO_SERVER__HOST` | `server.host` |
| `DYNDO_SERVER__PORT` | `server.port` |
| `DYNDO_FS__ROOT` | `fs.root` |
| `DYNDO_S3__BUCKET` | `s3.bucket` |
| `DYNDO_S3__REGION` | `s3.region` |
| `DYNDO_S3__ACCESS_KEY_ID` | `s3.access_key_id` |
| `DYNDO_S3__SECRET_ACCESS_KEY` | `s3.secret_access_key` |

The double-underscore rule is what keeps a name like `access_key_id` intact —
`DYNDO_S3__ACCESS_KEY_ID` nests as `s3.access_key_id`, not `s3.access.key.id`.

## Precedence

Later layers win field by field (a deep merge), so you can commit a
`config.yaml` and override only what changes per environment:

```bash
# config.yaml sets port 8080 and fs.root ./assets;
# these env vars override just those two fields.
DYNDO_SERVER__PORT=9000 DYNDO_FS__ROOT=/srv/media dyndo-server
```

## Startup errors

Configuration problems are reported at startup, not per request:

| Condition | Result |
|---|---|
| `DYNDO_CONFIG` names a missing file | Exit: "DYNDO_CONFIG points to a missing file". |
| YAML/env cannot be merged or deserialized into the schema | Exit: "failed to load configuration". |
| Selected backend rejected (e.g. `s3` with no `bucket`, `fs` with no `root`) | Exit: "failed to build storage operator". |
