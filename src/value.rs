pub enum ValueRef<'a> {
    ByteChunk(&'a [u8]),
    VarInt(i128)
}