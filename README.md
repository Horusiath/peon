# Prefixed Entry Object Notation

### Entry format

Each object is first flattened into a stream of key-value pairs, i.e. following structure:

```json
{
  "users": [
    {
      "alice": {
        "age": 30,
        "city": "Wonderland"
      }
    },
    {
      "bob": {
        "age": 25,
        "city": "Builderland"
      }
    }
  ]
}
```

which can be represented as:

```
$.users[0].alice.age = 30
$.users[0].alice.city = "Wonderland"
$.users[1].bob.age = 25
$.users[1].bob.city = "Builderland"
```

will be encoded using common prefixing:

```
18|0|.users.0.alice.age|30
20|15|city.0:10|Wonderland
16|7|1.bob.age|25
18|13|city.0:11|Builderland
```
where:
- 1st number is `u16` length of the key
- 2nd number is `u16` common prefix lenght between current key and previous one
- 3rd value is key itself, segmented as:
  - utf8 encoded string for keys
  - length prefixed varint for indexes
  - (always last) length prefixed varint + `u16` describing variable length values: single entry must never be longer than `u16::MAX`, in case if that happens it can be split into multiple entries with chunks of data.
- last value is either a byte slice or a varint with zigzag encoded value