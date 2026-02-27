using System.Collections;
using Un.Object.Function;
using Un.Object.Primitive;

namespace Un.Object.Collections;

public class Spreads(Obj[] values) : Ref<Obj[]>(values, "spread"), IEnumerable<Obj>
{
    public struct Enumerator(Spreads spread) : IEnumerator<Obj>
    {
        private readonly Obj[] arr = spread.Value;
        private int index = -1; 

        public readonly Obj Current => arr[index];

        readonly object IEnumerator.Current => arr[index];

        public bool MoveNext()
        {
            index++;
            return index < arr.Length;
        }

        public void Reset()
        {
            index = -1;
        }

        public void Dispose()
        {

        }
    }

    public int Count => Value.Length;

    public override Obj Spread() => this;
    
    public IEnumerator<Obj> GetEnumerator() => new Enumerator(this);

    IEnumerator IEnumerable.GetEnumerator() => new Enumerator(this);
}