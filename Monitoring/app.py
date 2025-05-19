from flask import Flask

app = Flask(__name__)

@app.route('/')
def hello_world():
    return 'Hello, World!'

@app.route("/data")
def data_page():
    return "This is data"

if __name__ == '__main__':
    app.run(debug=True)