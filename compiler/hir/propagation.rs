//! Table-aware structural validation, not a second ownership authority.
use super::*;
impl Propagation {
    pub(crate) fn validate(
        self,
        p: &Program,
        function: FunctionId,
        local: LocalId,
        initializer: &TypedExpr,
        span: Span,
    ) -> Result<(), &'static str> {
        let f = p
            .functions()
            .get(function.0)
            .filter(|f| f.id() == function)
            .and_then(Function::as_ordinary)
            .ok_or("ineligible propagation function")?;
        let l = p
            .locals()
            .get(local.0)
            .filter(|l| l.id == local && l.function == function)
            .ok_or("foreign propagation local")?;
        if self.span != span
            || f.return_type != Some(ValueType::Result(self.outer))
            || initializer.ty != ExprType::Result(self.inner)
            || p.result_payload(self.inner, ResultTag::Ok) != Some(l.ty)
            || p.result_payload(self.inner, ResultTag::Error) != Some(self.failure)
            || p.result_payload(self.outer, ResultTag::Error) != Some(self.failure)
        {
            return Err("propagation canonical type correspondence");
        }
        let b = self.syntax.binding;
        let phrase = self.syntax.phrase;
        if b.line == 0
            || b.column == 0
            || phrase.line == 0
            || phrase.column == 0
            || !(span.start <= b.start
                && b.start < b.end
                && b.end <= initializer.span.start
                && initializer.span.end <= phrase.start
                && phrase.start < phrase.end
                && phrase.end < span.end)
        {
            return Err("propagation source origin correspondence");
        }
        Ok(())
    }
}
