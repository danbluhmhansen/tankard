use maud::{html, Markup, DOCTYPE};

use crate::routes;

pub(crate) fn boost(content: Markup, signed_in: bool, boosted: bool) -> Markup {
    if boosted {
        html! {
            (header(signed_in))
            (main(content))
        }
    } else {
        full(content, signed_in)
    }
}

fn full(content: Markup, signed_in: bool) -> Markup {
    html! {
        (DOCTYPE)
        html hx-boost="true" hx-ext="alpine-morph" hx-swap="morph:innerHTML" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width,initial-scale=1";
                link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxZW0iIGhlaWdodD0iMWVtIiB2aWV3Qm94PSIwIDAgNTEyIDUxMiI+PHBhdGggZmlsbD0iY3VycmVudENvbG9yIiBkPSJNMjUyLjA5NCAxOS40MzhjLTE4LjA5Mi0uMDYzLTM1LjU0OCA5LjgyLTQzLjEyNSAyOC40Mzd2OS42ODhsLTExLjM3Ni0yLjVjLTE0LjMxNi0zLjE3LTI1Ljc5Mi0xLjE1LTMzLjM3NSAzLjg0M2MtNy41ODUgNC45OTQtMTIuMTc0IDEyLjg5OC0xMi4zNDUgMjUuNDM4Yy0uMTMgOS41NCAxLjkzIDE1LjgyIDQuODEzIDIwYzIuODgyIDQuMTggNi42NzMgNi42NzIgMTEuOTA2IDguMDYyYzEwLjQ2NSAyLjc4IDI2LjY3LS4zNTcgNDEuMDk0LTguNzVsNS45NjgtMy40N2w1LjA2MyA0LjY1OGM4LjQwNSA3Ljc0NCAxNC41MSAxMS4wNyAyMC41NiAxMi4yNWM2LjA1MiAxLjE4IDEzLjA0Ni4zMTggMjMuNDQtMi44NzVsOS44NDItMy4wMzJsMi4wNjMgMTAuMDkzYzIuNjk1IDEzLjE1OCAxNC45MSAyMy40MDcgMjkuMTI1IDIzLjQwN2MxMy4yMzcgMCAyMy42Ny05LjAyOCAyNy4zMTMtMjEuNDY4bDIuMjE4LTcuNTMybDcuNzgzLjg0M2M4Ljg1NS45OSAxOS40MS00LjA0NSAyNS0xMC4zNDNsNi02Ljc1bDYuOTY4IDUuNzgyYzE4LjYxIDE1LjQ4NyAzNS40NiAxNi45NiA0Ny4yODMgMTEuNDY4YzExLjgyLTUuNDk0IDIwLjE4LTE4LjYwMiAxOS4yNS0zOC43ODJjLS44OC0xOC44MjctMTAuOTctMzAuNDQ4LTI1LjUtMzUuODEyYy0xNC41MzItNS4zNjQtMzMuNzYtMy42MS01MS4yODIgOC4yMThsLTcuNDM2IDUuMDMybC01LjM0NC03LjI1Yy03LjAzOC05LjU4NS0xNy4wOS0xNS40ODUtMjYuNzItMTdjLTkuNjI4LTEuNTE2LTE4LjQ4Ny45MjgtMjUuMzc0IDguNDA2bC03LjQwNiA4LjAzbC02Ljc4LTguNTZjLTEwLjQ0My0xMy4xNjUtMjUuMjE0LTE5LjQ4Mi0zOS42MjYtMTkuNTMyek02NS4yMiAxMTkuOTY4QzM3LjggMjAzLjY1IDI1Ljc4NCAyODkuMDcgMjguODEyIDM3Ni4xOWMzOS41NSAxNy4yMyA4MS40MjIgMTguMTA1IDEyMy40MzcgMThhOTU2LjU4OCA5NTYuNTg4IDAgMCAwIDYuNTk0LTM0LjIyYy0zMi4xMDIgMS42NzgtNjQuMDk0IDIuNTItOTQuMzEzLTkuMTI0Yy0yLjMzLTY2Ljg4IDYuOTE3LTEyMS42MjIgMjgtMTg3LjAzYzI3LjMxOCA2LjUgNTUuMDEgOC42MSA4My4yNSA3LjQ2N2MtLjA3LTExLjcxNS0uMzg3LTIyLjU1Ni0xLjAzLTMyLjMxYy0zNy4xNjgtMS43MjYtNzMuNTkzLTguNjQyLTEwOS41My0xOXptMTQ4IDIuOTdjLTYuNTcgMy4yOS0xMy4zNyA1LjgyLTIwLjE5IDcuNDA2YzMuMDkyIDMzLjQ1NiAxLjk0NyA3OC4zOTItMi4xODYgMTI3LjA5NGMtNC43NzcgNTYuMjgtMTMuODY2IDExNi41LTI2LjQzOCAxNjYuNzE4SDQzNC4yNWMtOS45MzItNTIuNTY1LTE4LjgxMi0xMTEuNjEtMjMuNTk0LTE2Ni43MmMtMy44Ny00NC42MTgtNS4yMzMtODYuMTE1LTIuMDMtMTE5LjcxN2MtMTAuNzc3LTEuMjgyLTIyLjA0Ny01LjY0Mi0zMi45MzgtMTMuMjJjLTcuNDk4IDUuOTg4LTE2Ljk1NCAxMC4xNDUtMjcuMjUgMTAuNzVjLTcuNDYgMTYuMjQ3LTIzLjQyIDI4LjEyNS00Mi42ODggMjguMTI1Yy0xOS42NDQgMC0zNi44NC0xMS44Ni00NC4zNDQtMjguOTM4Yy04LjI2IDEuODg1LTE1Ljk5MyAyLjUwNy0yMy43MiAxYy04LjU3LTEuNjctMTYuNDY4LTYuMDE0LTI0LjQ2Ny0xMi41em0tNzguMzc2IDMxOS45MDZMMTE2LjIyIDQ5MS4yNWgzNTguNjg2bC0yMS43Mi00OC40MDZoLTMxOC4zNHoiLz48L3N2Zz4=";
                link rel="stylesheet" href="site.css";
                script type="module" src="index.js" {}
                script type="module" src="alpine.js" {}
                script type="module" src="htmx.js" {}
            }
            body { (header(signed_in)) (main(content)) }
        }
    }
}

fn header(signed_in: bool) -> Markup {
    html! {
        header {
            nav {
                ul { li { a href=(routes::index::Path) { "Tankard" } } }
                ul {
                    @if signed_in {
                        li { a href=(routes::games::Path) { "Games" } }
                    }
                }
                ul {
                    @if signed_in {
                        li { a href=(routes::profile::Path) { "Profile" } }
                        li { form action=(routes::signout::Path) method="post" { button type="submit" { "Sign out" } } }
                    } @else {
                        li { a href=(routes::signup::Path) { "Sign up" } }
                        li { a href=(routes::signin::Path) { "Sign in" } }
                    }
                }
            }
        }
    }
}

fn main(content: Markup) -> Markup {
    html! { main { (content) } }
}
