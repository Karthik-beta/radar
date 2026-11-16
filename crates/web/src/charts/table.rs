use leptos::prelude::*;

/// The visually hidden table every chart carries for a screen reader. The first
/// cell of each row is its row header. One function serves every chart, so the
/// table markup is compiled once.
#[inline(never)]
pub fn data_table(caption: String, head: Vec<String>, rows: Vec<Vec<String>>) -> AnyView {
    let head = head
        .into_iter()
        .map(|text| view! { <th scope="col">{text}</th> })
        .collect_view();
    let body = rows
        .into_iter()
        .map(|row| {
            let mut cells = row.into_iter();
            let first = cells.next().unwrap_or_default();
            let rest = cells.map(|text| view! { <td>{text}</td> }).collect_view();
            view! {
                <tr>
                    <th scope="row">{first}</th>
                    {rest}
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class="sr-only"><table>
            <caption>{caption}</caption>
            <thead>
                <tr>{head}</tr>
            </thead>
            <tbody>{body}</tbody>
        </table></div>
    }
    .into_any()
}
